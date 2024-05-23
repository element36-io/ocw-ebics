//! Fiat on-off ramps offchain worker
//!
//! Polls Nexus API at a given interval to get the latest bank statement and
//! updates the onchain state accordingly.
#![cfg_attr(not(feature = "std"), no_std)]
#![allow(clippy::type_complexity)]
extern crate alloc;

use alloc::{collections::BTreeMap, vec::Vec};
use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{
	dispatch::DispatchResultWithPostInfo,
	ensure,
	pallet_prelude::*,
	traits::{
		BuildGenesisConfig, Currency, ExistenceRequirement, Get, LockableCurrency, UnixTime,
		WithdrawReasons,
	},
};
use frame_system::{
	ensure_signed,
	offchain::{
		AppCrypto, CreateSignedTransaction, SendSignedTransaction, SignedPayload, Signer,
		SigningTypes,
	},
	pallet_prelude::*,
};
use lite_json::{json::JsonValue, parse_json, Serialize};
use risc0_zkvm::{InnerReceipt, Receipt};
use scale_info::{prelude::format, TypeInfo};
use sp_core::hexdisplay::AsBytesRef;
use sp_runtime::{
	offchain as rt_offchain,
	offchain::storage::{MutateStorageError, StorageRetrievalError, StorageValueRef},
	traits::{AccountIdConversion, Zero},
	transaction_validity::{InvalidTransaction, TransactionValidity},
	AccountId32, SaturatedConversion,
};
use sp_std::{
	convert::{TryFrom, TryInto},
	default::Default,
	fmt::Debug,
	vec,
};

use sp_core::crypto::Ss58Codec;

pub mod crypto;
pub(crate) mod impls;
pub mod types;
mod utils;

#[cfg(test)]
mod helpers;
#[cfg(test)]
pub mod mock;
#[cfg(test)]
mod tests;

use crate::types::*;
use utils::*;

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use types::StringOf;

	// pub enum TransactType {
	// 	Mint(BalanceOf<T>),
	// 	Burn(BalanceOf<T>),
	// 	Transfer(AccountIdOf<T>, BalanceOf<T>),
	// 	RegisterForeignAsset(BalanceOf<T>),
	// }

	// pub trait EncodeAssetHubTransactCall {
	// 	fn encode_asset_hub_transact_call(&self, transact_type: TransactType) -> Vec<u8>;
	// }

	#[pallet::pallet]
	#[pallet::without_storage_info]
	pub struct Pallet<T>(_);

	/// This is the pallet's trait
	#[pallet::config]
	pub trait Config: frame_system::Config + CreateSignedTransaction<Call<Self>> {
		/// The identifier type for an offchain SendSignedTransaction
		type AuthorityId: AppCrypto<Self::Public, Self::Signature>;

		/// The overarching event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// Loosely coupled timestamp provider
		type TimeProvider: UnixTime;

		/// Currency type for stablecoin
		type Stablecoin: Currency<Self::AccountId> + LockableCurrency<Self::AccountId>;

		/// Maximum number of characters in IBAN
		#[pallet::constant]
		type MaxIbanLength: Get<u32> + PartialEq + Eq + MaxEncodedLen + TypeInfo + Clone + Debug;

		/// Maximum number of characters string type in general
		#[pallet::constant]
		type MaxStringLength: Get<u32> + PartialEq + Eq + MaxEncodedLen + TypeInfo + Clone + Debug;

		/// This ensures that we only accept unsigned transactions once, every `UnsignedInterval` blocks.
		#[pallet::constant]
		type MinimumInterval: Get<u64>;

		/// A configuration for base priority of unsigned transactions.
		///
		/// This is exposed so that it can be tuned for particular runtime, when
		/// multiple pallets send unsigned transactions.
		#[pallet::constant]
		type UnsignedPriority: Get<u64>;

		/// OCW account
		type OcwAccount: Get<Self::AccountId>;

		/// Max number of statements to process
		#[pallet::constant]
		type MaxStatements: Get<u32> + PartialEq + Eq + MaxEncodedLen + TypeInfo + Clone;

		/// AccountId to `MultiLocation` conversion
		type XcmInterface: XcmInterface<Self::RuntimeOrigin, BalanceOf<Self>, AccountIdOf<Self>>;
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn offchain_worker(block_number: BlockNumberFor<T>) {
			log::info!("[OCW] Instantiating offchain worker");

			let parent_hash = <frame_system::Pallet<T>>::block_hash(block_number - 1u32.into());
			log::debug!("[OCW] Current block: {:?} (parent hash: {:?})", block_number, parent_hash);

			let should_sync = Self::should_sync();

			log::info!("[OCW] Syncing: {}", &should_sync);

			if !should_sync {
				log::error!("[OCW] Too early to sync");
				return;
			}

			// Choose which activity to perform
			let activity = Self::choose_ocw_activity();

			log::info!("[OCW] Current activity: {:?}", &activity);

			let res = match activity {
				OcwActivity::FetchStatements | OcwActivity::VerifyAndProcessStatements =>
					Self::fetch_and_send_signed(activity),
				OcwActivity::ProcessBurnRequests => Self::process_burn_requests(),
				_ => {
					log::error!("[OCW] No activity to perform");
					Ok(())
				},
			};

			if let Err(e) = res {
				log::error!("[OCW] Error syncing with bank: {}", e);
			} else {
				let now = T::TimeProvider::now();
				log::info!("[OCW] Last sync timestamp: {}", now.as_millis());
			}
		}
	}

	/// Counts the number of burn requests, irrespective of the sender and burn request status
	#[pallet::storage]
	#[pallet::getter(fn burn_request_count)]
	pub(super) type BurnRequestCount<T: Config> = StorageValue<_, u64, ValueQuery>;

	#[pallet::type_value]
	pub(super) fn DefaultApi<T: Config>() -> StringOf<T> {
		StringOf::<T>::try_from(API_URL.to_vec())
			.expect("Might fail if T::MaxStringLength is less than 33")
	}

	/// URL of the API endpoint
	#[pallet::storage]
	pub(super) type ApiUrl<T: Config> = StorageValue<_, StringOf<T>, ValueQuery, DefaultApi<T>>;

	/// Mapping from `AccountId` to `BankAccount`
	#[pallet::storage]
	#[pallet::getter(fn account_of)]
	pub(super) type Accounts<T: Config> =
		StorageMap<_, Blake2_128Concat, AccountIdOf<T>, BankAccountOf<T>>;

	/// Stores burn requests
	/// until they are confirmed by the bank as outgoing transaction
	/// transaction_id -> burn_request
	#[pallet::storage]
	#[pallet::getter(fn burn_requests)]
	pub(super) type BurnRequests<T: Config> =
		StorageMap<_, Blake2_128Concat, u64, BurnRequest<T::MaxIbanLength, BalanceOf<T>>>;

	/// Stores latest statements waiting for verification
	///
	/// Maps `receipt_url` to `Statements`
	#[pallet::storage]
	#[pallet::getter(fn queued_statements)]
	pub(super) type QueuedStatements<T: Config> =
		StorageValue<_, QueuedStatementsInfoOf<T>, OptionQuery>;

	/// Image ID of the risc0 module
	#[pallet::storage]
	#[pallet::getter(fn risc0_image_id)]
	pub(super) type Risc0ImageId<T: Config> = StorageValue<_, [u8; 32], OptionQuery>;

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Set api url for fetching bank statements
		// TO-DO change weight for appropriate value
		#[pallet::weight(T::DbWeight::get().reads_writes(2, 1))]
		#[pallet::call_index(0)]
		pub fn set_api_url(origin: OriginFor<T>, url: StringOf<T>) -> DispatchResultWithPostInfo {
			ensure_root(origin)?;
			<ApiUrl<T>>::put(url);
			Ok(().into())
		}

		/// Create new bank account instance
		///
		/// # Arguments
		///
		/// * `origin` - The origin of the call
		/// * `iban` - IBAN of the account
		#[pallet::weight(T::DbWeight::get().reads_writes(2, 1))]
		#[pallet::call_index(1)]
		pub fn create_account(origin: OriginFor<T>, iban: IbanOf<T>) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;

			// TO-DO: need to check if account owner really owns this IBAN
			Accounts::<T>::insert(
				&who,
				BankAccount::<T::MaxIbanLength> {
					iban: iban.clone(),
					balance: 0u128,
					last_updated: T::TimeProvider::now().as_millis() as u64,
				},
			);

			Self::deposit_event(Event::AccountCreated(who, iban));

			Ok(().into())
		}

		/// Remove `IbanAccount` from the store
		///
		/// # Arguments
		///
		/// `iban`: IbanAccount struct
		#[pallet::weight(T::DbWeight::get().reads_writes(2, 1))]
		#[pallet::call_index(2)]
		pub fn unmap_iban_account(
			origin: OriginFor<T>,
			iban: IbanOf<T>,
		) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;

			Accounts::<T>::remove(&who);

			Self::deposit_event(Event::AccountDestroyed(who, iban));

			Ok(().into())
		}

		/// Single transfer extrinsic which works for 3 different types of transfers:
		///
		/// 1. Withdrawal of on-chain funds to the linked IBAN account in `Accounts`
		/// 2. Transfer to specified IBAN account. This will try to find the linked on-chain account and
		///   transfer to it if it exists, otherwise it will transfer to the IBAN account off-chain
		/// 3. Transfer to the on-chain address
		///
		/// # Arguments
		///
		/// `amount`: Amount of tokens to burn
		/// `iban`: IbanOf<T> account of the receiver
		/// `dest`: `TransferDestination` enum which can be either `Iban`, `AccountId` or withdrawal
		#[pallet::weight(T::DbWeight::get().reads_writes(10, 2))]
		#[pallet::call_index(3)]
		pub fn transfer(
			origin: OriginFor<T>,
			amount: BalanceOf<T>,
			dest: TransferDestinationOf<T>,
		) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;

			ensure!(T::Stablecoin::free_balance(&who) >= amount, Error::<T>::InsufficientBalance,);

			ensure!(!amount.is_zero(), Error::<T>::AmountIsZero);

			// Request id (nonce)
			let request_id = Self::burn_request_count();

			// Get bank account associated with the sender
			let source_account = Accounts::<T>::get(&who).ok_or(Error::<T>::AccountNotFound)?;

			let dest_iban = match dest.clone() {
				TransferDestination::Iban(iban) => Ok(iban),
				TransferDestination::Address(dest_account) => Accounts::<T>::get(&dest_account)
					.ok_or(Error::<T>::AccountNotFound)
					.map(|account| account.iban),
				TransferDestination::Withdraw => Ok(source_account.iban.clone()),
			}?;

			let burn_request = BurnRequest {
				id: request_id,
				burner: source_account.iban,
				dest_iban: dest_iban.clone(),
				amount,
			};

			// Create new burn request in the storage
			<BurnRequests<T>>::insert(request_id, burn_request);

			// Increase burn request count
			<BurnRequestCount<T>>::put(request_id + 1);

			// Extract destination account from iban
			let dest_address = match dest {
				TransferDestination::Withdraw => Some(who.clone()),
				_ => Self::get_account_id(&dest_iban),
			};

			// transfer amount to this pallet's account
			T::Stablecoin::transfer(
				&who,
				&Self::account_id(),
				amount,
				ExistenceRequirement::AllowDeath,
			)?;

			// create burn request event
			Self::deposit_event(Event::BurnRequest {
				request_id,
				burner: who,
				dest: dest_address,
				dest_iban,
				amount,
			});

			Ok(().into())
		}

		/// Processes new statements
		///
		/// This is used to process transactions in the bank statement
		///
		/// NOTE: This call can be called only by the offchain worker
		/// Params:
		///
		/// `statements`: list of statements to process
		/// 	`iban_account`: IBAN account connected to the statement
		/// 	`Vec<Transaction>`: List of transactions to process
		#[pallet::weight(T::DbWeight::get().reads_writes(T::MaxStatements::get() as u64, 1))]
		#[pallet::call_index(4)]
		pub fn process_statements(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
			// this can be called only by the ocw account
			let who = ensure_signed(origin)?;
			ensure!(who == T::OcwAccount::get(), Error::<T>::UnauthorizedCall);

			let statements_info = QueuedStatements::<T>::take().ok_or(Error::<T>::IbanNotMapped)?;

			log::info!("[OCW] Processing statements");

			for (iban_account, transactions) in statements_info.clone().statements {
				let should_process = Self::should_process_transactions(&iban_account);

				if should_process {
					Self::process_transactions(&iban_account, &transactions)?;
				}
			}

			Self::deposit_event(Event::ProofVerified { statements_info });

			Ok(().into())
		}

		/// Queue new statements for processing
		#[pallet::weight(T::DbWeight::get().writes(1))]
		#[pallet::call_index(5)]
		pub fn queue_statements(
			origin: OriginFor<T>,
			receipt_url: StringOf<T>,
			statements: StatementsOf<T>,
		) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;
			log::info!("who {:?}", who);

			ensure!(who == T::OcwAccount::get(), Error::<T>::UnauthorizedCall);

			QueuedStatements::<T>::put(QueuedStatementsInfo {
				block_number: frame_system::Pallet::<T>::block_number(),
				receipt_url: receipt_url.clone(),
				statements,
			});

			log::info!("[OCW] Queued statements for processing");

			Ok(().into())
		}

		/// Set image id of the risc0 module
		#[pallet::weight(T::DbWeight::get().writes(1))]
		#[pallet::call_index(6)]
		pub fn set_risc0_image_id(
			origin: OriginFor<T>,
			image_id: [u8; 32],
		) -> DispatchResultWithPostInfo {
			ensure_root(origin)?;

			Risc0ImageId::<T>::put(image_id);

			Ok(().into())
		}

		/// Teleport pEUR to the Asset Hub
		///
		/// If `dest` is None, the amount will be teleported to the sender's account
		#[pallet::weight(T::DbWeight::get().reads(1))]
		#[pallet::call_index(7)]
		pub fn teleport_to_asset_hub(
			origin: OriginFor<T>,
			amount: BalanceOf<T>,
			dest: Option<AccountIdOf<T>>,
		) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin.clone())?;

			ensure!(T::Stablecoin::free_balance(&who) >= amount, Error::<T>::InsufficientBalance,);

			// Transfer amount to the Asset Hub
			T::XcmInterface::mint(origin, amount, dest.unwrap_or(who))?;

			Ok(().into())
		}
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// New IBAN has been mapped to an account
		AccountCreated(T::AccountId, IbanOf<T>),
		/// IBAN has been un-mapped from an account
		AccountDestroyed(T::AccountId, IbanOf<T>),
		/// New minted tokens to an account
		Minted { who: T::AccountId, iban: IbanOf<T>, amount: BalanceOf<T> },
		/// New burned tokens from an account
		Burned { who: T::AccountId, iban: IbanOf<T>, amount: BalanceOf<T> },
		/// New Burn request has been made
		BurnRequest {
			request_id: u64,
			burner: T::AccountId,
			dest: Option<T::AccountId>,
			dest_iban: IbanOf<T>,
			amount: BalanceOf<T>,
		},
		/// Transfer event with IBAN numbers
		Transfer { from: IbanOf<T>, to: IbanOf<T>, amount: BalanceOf<T> },
		/// Statement processed
		StatementProcessed {
			holder: AccountIdOf<T>,
			iban: IbanOf<T>,
			failed_transactions: Vec<u32>,
		},
		/// ZK proof verified
		ProofVerified {
			statements_info: QueuedStatementsInfo<BlockNumberFor<T>, StatementsOf<T>, StringOf<T>>,
		},
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Account is not mapped to an IBAN
		AccountNotMapped,
		/// IBAN is not mapped to an account
		IbanNotMapped,
		/// Account not found when trying to map IBAN
		AccountNotFound,
		/// Amount is zero
		AmountIsZero,
		/// Insufficient funds
		InsufficientBalance,
		/// Unauthorized call for `process_statements`. Only ocw account can call this
		UnauthorizedCall,
		/// Unroutable XCM message
		UnroutableXcm,
		/// Generic XCM error
		XcmError,
	}

	#[pallet::validate_unsigned]
	impl<T: Config> ValidateUnsigned for Pallet<T> {
		type Call = Call<T>;
		fn validate_unsigned(_source: TransactionSource, call: &Self::Call) -> TransactionValidity {
			if let Call::process_statements {} = call {
				Self::validate_tx_parameters()
			} else {
				InvalidTransaction::Call.into()
			}
		}
	}

	/// Genesis configuration
	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config> {
		pub accounts: Vec<(T::AccountId, Vec<u8>)>,
	}
	impl<T: Config> Default for GenesisConfig<T> {
		fn default() -> Self {
			GenesisConfig { accounts: vec![] }
		}
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			for (account, iban) in &self.accounts {
				Accounts::<T>::insert(
					account,
					BankAccount {
						iban: iban.clone().try_into().expect("IBAN should be valid"),
						balance: 0u128,
						last_updated: T::TimeProvider::now().as_millis() as u64,
					},
				);
			}
		}
	}
}
