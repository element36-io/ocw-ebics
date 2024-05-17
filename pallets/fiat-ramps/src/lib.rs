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
mod impls;
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

	#[pallet::pallet]
	#[pallet::without_storage_info]
	pub struct Pallet<T>(_);

	/// This is the pallet's trait
	#[pallet::config]
	pub trait Config:
		frame_system::Config + CreateSignedTransaction<Call<Self>> + pallet_sudo::Config
	{
		/// The identifier type for an offchain SendSignedTransaction
		type AuthorityId: AppCrypto<Self::Public, Self::Signature>;

		/// The overarching event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// Loosely coupled timestamp provider
		type TimeProvider: UnixTime;

		/// Currency type
		type Currency: Currency<Self::AccountId> + LockableCurrency<Self::AccountId>;

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
				return
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

			ensure!(T::Currency::free_balance(&who) >= amount, Error::<T>::InsufficientBalance,);

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
			T::Currency::transfer(
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

/// Utility functions
impl<T: Config> Pallet<T> {
	/// AccountId associated with Pallet
	fn account_id() -> T::AccountId {
		PALLET_ID.try_into_account().expect("Should never fail")
	}

	/// Returns the action to take
	fn choose_ocw_activity() -> OcwActivity {
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
	fn should_sync() -> bool {
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
	fn should_process_transactions(_iban_account: &BankAccountOf<T>) -> bool {
		true
	}

	/// Checks if iban is mapped to an account in the storage
	fn iban_exists(iban: &IbanOf<T>) -> bool {
		Accounts::<T>::iter().any(|(_, v)| &v.iban == iban)
	}

	/// Extract AccountId mapped to IbanOf<T>
	fn get_account_id(iban: &IbanOf<T>) -> Option<T::AccountId> {
		Accounts::<T>::iter().find(|(_, v)| &v.iban == iban).map(|(k, _)| k)
	}

	/// Ensures that an IBAN  is mapped to an account in the storage
	///
	/// If necessary, creates new account
	fn ensure_iban_is_mapped(iban: &IbanOf<T>) -> AccountIdOf<T> {
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
}

/// Functions related to statement processing
impl<T: Config> Pallet<T> {
	/// Process a single transaction
	///
	/// ### Arguments
	///
	/// - `statement_owner`: Owner of the statement we are processing
	/// - `statement_iban`: IBAN number of the statement we are processing
	/// - `source`: Source/sender of the transaction
	/// - `dest`: Destination/receiver of the transaction
	/// - `transaction`: Transaction data
	/// - `reference`: Optional reference field (usually contains burn request id)
	fn process_transaction(
		statement_owner: &AccountIdOf<T>,
		statement_iban: &IbanOf<T>,
		source: Option<T::AccountId>,
		dest: Option<T::AccountId>,
		transaction: &TransactionOf<T>,
		reference: Option<u64>,
	) -> DispatchResult {
		let amount: BalanceOf<T> = BalanceOf::<T>::try_from(transaction.amount).unwrap_or_default();

		// Process transaction based on its type
		match transaction.tx_type {
			TransactionType::Incoming => {
				log::debug!("[OCW] Incoming transaction {:?}", &transaction);
				if let Some(sender) = source {
					T::Currency::transfer(
						&sender,
						statement_owner,
						amount,
						ExistenceRequirement::AllowDeath,
					)?;
				} else {
					// Sender is not on-chain, therefore we simply mint to statement owner
					log::info!("[OCW] Mint to {:?} {:?}", statement_owner, amount.clone());

					// Returns negative imbalance
					let mint = T::Currency::issue(amount);

					// deposit negative imbalance into the account
					T::Currency::resolve_creating(statement_owner, mint);

					Self::deposit_event(Event::Minted {
						who: statement_owner.clone(),
						iban: statement_iban.clone(),
						amount,
					});
				}
			},
			TransactionType::Outgoing => {
				log::debug!("[OCW] Outgoing transaction {:?}", &transaction);

				let pallet_account = Self::account_id();
				let burn_request = reference.and_then(BurnRequests::<T>::take);

				if dest.is_some() {
					// Here we get the actual sender and receiver of the transaction
					// If user has submitted burn request, his funds are stored in the pallet's account
					// and if we detect that the reference field is populated with a burn request id,
					// we can transfer the funds from the pallet's account to the specified destination
					// account specified in the burn request.
					//
					// Otherwise, we simply transfer the funds from the statement owner to the receiver
					if let Some(request) = burn_request {
						match (
							Self::get_account_id(&request.burner),
							Self::get_account_id(&request.dest_iban),
						) {
							(Some(from), Some(to)) => {
								if from == to {
									// user is burning funds to himself, meaning he is withdrawing from ihis bank account

									// Returns negative imbalance
									let burn = T::Currency::burn(amount);
									// Burn negative imbalance from the account
									if T::Currency::settle(
										&pallet_account,
										burn,
										WithdrawReasons::TRANSFER,
										ExistenceRequirement::AllowDeath,
									)
									.is_ok()
									{
										Self::deposit_event(Event::Burned {
											who: pallet_account,
											iban: transaction.iban.clone(),
											amount,
										});
									}
								} else {
									// both sides are on-chain, so we can simply transfer the funds
									T::Currency::transfer(
										&from,
										&to,
										amount,
										ExistenceRequirement::AllowDeath,
									)?;
								}
							},
							(Some(_account), None) => {
								// user is sending to an unknown account, so we burn the funds
								// Returns negative imbalance
								let burn = T::Currency::burn(amount);
								// Burn negative imbalance from the account
								if T::Currency::settle(
									&pallet_account,
									burn,
									WithdrawReasons::TRANSFER,
									ExistenceRequirement::AllowDeath,
								)
								.is_ok()
								{
									Self::deposit_event(Event::Burned {
										who: pallet_account,
										iban: transaction.iban.clone(),
										amount,
									});
								}
							},
							_ => {},
						}
					} else {
						// Receiver is on-chain, therefore we simply transfer from statement owner
						T::Currency::transfer(
							statement_owner,
							&dest.unwrap(),
							amount,
							ExistenceRequirement::AllowDeath,
						)?;
					}
				} else {
					// Receiver is not on-chain, therefore we simply burn from statement owner
					log::info!("[OCW] Burn from {:?} {:?}", pallet_account, amount.clone());
					let burn_from = if burn_request.is_some() {
						pallet_account.clone()
					} else {
						statement_owner.clone()
					};

					// Returns negative imbalance
					let burn = T::Currency::burn(amount);

					// Burn negative imbalance from the account
					if T::Currency::settle(
						&burn_from,
						burn,
						WithdrawReasons::TRANSFER,
						ExistenceRequirement::AllowDeath,
					)
					.is_ok()
					{
						Self::deposit_event(Event::Burned {
							who: burn_from,
							iban: transaction.iban.clone(),
							amount,
						});
					}
				}
			},
			_ => {},
		}

		Ok(())
	}

	/// Process list of transactions for a given iban account
	///
	/// # Arguments
	///
	/// `iban: IbanAccount` - iban account to process transactions for
	/// `transactions: Vec<Transaction>` - list of transactions to process
	fn process_transactions(
		iban_account: &BankAccountOf<T>,
		#[allow(clippy::ptr_arg)] transactions: &Vec<TransactionOf<T>>,
	) -> DispatchResult {
		// Get account id of the statement owner
		let statement_owner = Self::ensure_iban_is_mapped(&iban_account.iban);

		// contains index of transaction that failed
		let mut failed_transactions: Vec<u32> = vec![];

		// Iterate over all transactions, with index

		for (i, transaction) in transactions.iter().enumerate() {
			// decode destination account id from reference
			let reference_str = core::str::from_utf8(&transaction.reference).unwrap_or("default");

			// Format of the reference is the following:
			// Purpose:AccountId; ourReference:nonce(of burn request)
			// E.g, "Purp:5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty; ourRef:12",
			let reference_decoded: Vec<&str> = reference_str.split(';').collect();

			log::info!("[OCW] Purpose: {}", reference_decoded[0]);
			log::info!("[OCW] Reference: {}", reference_decoded[1]);

			// Source (initiator) of the transaction
			let source: Option<AccountIdOf<T>> = match transaction.tx_type {
				TransactionType::Incoming => Self::get_account_id(&transaction.iban),
				TransactionType::Outgoing => Self::get_account_id(&iban_account.iban),
				_ => None,
			};

			// Destination (recipient) of the transaction
			let dest = match AccountId32::from_ss58check(&reference_decoded[0][5..]) {
				Ok(dest) => Some(
					<T::AccountId>::decode(&mut &dest.encode()[..]).expect("Should not fail; qed"),
				),
				Err(_e) => {
					log::error!("[OCW] Failed to decode destination account from reference");
					match transaction.tx_type {
						TransactionType::Incoming => Self::get_account_id(&iban_account.iban),
						TransactionType::Outgoing => Self::get_account_id(&transaction.iban),
						_ => None,
					}
				},
			};

			// Proces transaction based on the value of reference
			// If decoding returns error, we look for the iban in the pallet storage
			let reference = reference_decoded[1][8..].parse::<u64>().ok();

			if Self::process_transaction(
				&statement_owner,
				&iban_account.iban,
				source,
				dest,
				transaction,
				reference,
			)
			.is_err()
			{
				failed_transactions.push(i as u32);
			}
		}

		Self::deposit_event(Event::StatementProcessed {
			holder: statement_owner,
			iban: iban_account.iban.clone(),
			failed_transactions,
		});

		Ok(())
	}

	/// Send unpeq request to the remote endpoint
	/// Populates the unpeg request and sends it
	///
	/// Note: This function is not called from the runtime, but from the OCW module
	///
	/// ### Arguments
	/// * `request_id` - id of the request to send
	/// * `burner` - AccountId of the burner, used to populate `purpose` field
	/// * `dest_iban` - IBAN destination
	/// * `amount` - Amount to be burned
	fn unpeg(
		request_id: u64,
		burner: Option<AccountIdOf<T>>,
		dest_iban: IbanOf<T>,
		amount: BalanceOf<T>,
	) -> Result<(), &'static str> {
		let remote_url = ApiUrl::<T>::get();

		let remote_url_str = core::str::from_utf8(&remote_url[..])
			.map_err(|_| "Error in converting remote_url to string")?;

		// add /unpeg to the url
		let remote_url_str = format!("{}/api-v1/unpeg", remote_url_str);

		let amount_u128 = amount.saturated_into::<u128>();

		// In the reference field, we save the request id (nonce)
		let reference = format!("{}", request_id);

		// Send request to remote endpoint
		let body = unpeg_request::<T>(
			&format!(
				"{:?}",
				AccountId32::try_from(burner.unwrap_or_else(Self::account_id).encode().as_ref())
					.expect("Should not fail")
					.to_ss58check()
			),
			amount_u128,
			&dest_iban,
			&reference,
		)
		.serialize();

		log::info!("[OCW] Sending unpeg request to {}", remote_url_str);

		let post_request = rt_offchain::http::Request::new(&remote_url_str)
			.method(rt_offchain::http::Method::Post)
			.body(vec![body])
			.add_header("Content-Type", "application/json")
			.add_header("accept", "*/*")
			.send()
			.map_err(|_| "Error in sending http POST request")?;

		log::info!("[OCW] Request sent to {}", remote_url_str);

		let response = post_request.wait().map_err(|_| "Error in waiting http response back")?;

		log::info!("[OCW] Unpeg response received {:?}", response.code);

		if response.code != 200 {
			return Err("Error in unpeg response")
		}

		Ok(())
	}

	/// Process burn requets
	///
	/// Processes registered burn requests, by sending http call to `unpeg` endpoint
	fn process_burn_requests() -> Result<(), &'static str> {
		for (request_id, burn_request) in <BurnRequests<T>>::iter() {
			// This is a default value, should not be processed
			if burn_request.burner == IbanOf::<T>::default() {
				return Ok(())
			}

			// Process burn requests that are either not processed yet or failed
			let dest_account = Self::get_account_id(&burn_request.dest_iban);

			// send the unpeg request
			match Self::unpeg(request_id, dest_account, burn_request.dest_iban, burn_request.amount)
			{
				Ok(_) => {
					log::info!("[OCW] Unpeq request successfull");
					BurnRequests::<T>::remove(request_id);
				},
				Err(e) => {
					log::info!("[OCW] Unpeq request failed {}", e);
					BurnRequests::<T>::remove(request_id);
				},
			};
		}

		Ok(())
	}
}

/// Functions related to the offchain worker communication
impl<T: Config> Pallet<T> {
	/// Fetch json from the Ebics Service API
	/// Return parsed json file
	fn fetch_json(endpoint: &str) -> Result<JsonValue, &'static str> {
		// fetch json value
		let remote_url = ApiUrl::<T>::get();
		let remote_url_str = core::str::from_utf8(remote_url.as_bytes_ref())
			.map_err(|_| "Error in converting remote_url to string")?;

		let final_url = format!("{}/{}", remote_url_str, endpoint);
		let pending = rt_offchain::http::Request::get(&final_url)
			.send()
			.map_err(|_| "Error in sending http GET request")?;

		let response = pending.wait().map_err(|_| "Error in waiting http response back")?;

		if response.code != 200 {
			// runtime_print!("Unexpected status code: {}", response.code);
			return Ok(JsonValue::Null)
		}

		let json_result: Vec<u8> = response.body().collect::<Vec<u8>>();

		let json_str: &str = match core::str::from_utf8(&json_result) {
			Ok(v) => v,
			Err(_e) => "Error parsing json",
		};

		log::info!("[OCW] JSON received: {}", json_str);

		let json_val = parse_json(json_str).map_err(|_| "Error in parsing json")?;

		Ok(json_val)
	}
	fn fetch_and_send_signed(ocw_activity: OcwActivity) -> Result<(), &'static str> {
		log::info!("[OCW] Activity: {:?}", ocw_activity);

		// get extrinsic signer
		let signer = Signer::<T, T::AuthorityId>::all_accounts();

		if !signer.can_sign() {
			return Err("No local accounts available! Please, insert your keys!")
		}

		let image_id = Risc0ImageId::<T>::get().ok_or("No image id set")?;

		// Get statements from remote endpoint
		let results = match ocw_activity {
			OcwActivity::FetchStatements => {
				let statements = Self::parse_statements()?;
				if statements.is_empty() {
					return Ok(())
				}

				let receipt_url = statements.keys().next().expect("Should not be empty");
				let statements = statements.values().cloned().collect::<Vec<(
					BankAccountOf<T>,
					BoundedVec<TransactionOf<T>, <T as Config>::MaxStatements>,
				)>>();

				Some(signer.send_signed_transaction(|_account| Call::queue_statements {
					statements:
						statements.clone().try_into().expect("convert to bounded vec, is fine"),
					receipt_url: receipt_url.clone(),
				}))
			},
			OcwActivity::VerifyAndProcessStatements => {
				let receipt_url = QueuedStatements::<T>::get()
					.ok_or("No queued statements")?
					.receipt_url
					.into_inner();

				let receipt_url = core::str::from_utf8(&receipt_url[..])
					.map_err(|_| "Error in converting receipt_url to string")?;

				let receipt = Self::fetch_json(receipt_url)?;

				if receipt.is_null() {
					return Ok(())
				}

				let (receipt, _): (Receipt, _) = serde_json_core::from_slice(&receipt.serialize())
					.map_err(|_| "Error in deserializing receipt")?;

				match receipt.verify(image_id) {
					Ok(_) =>
						Some(signer.send_signed_transaction(|_account| Call::process_statements {})),
					Err(e) =>
						if matches!(receipt.inner, InnerReceipt::Fake { .. }) {
							#[cfg(any(feature = "risc0-dev-mode", test))]
							{
								Some(signer.send_signed_transaction(|_account| {
									Call::process_statements {}
								}))
							}
							#[cfg(not(any(feature = "risc0-dev-mode", test)))]
							{
								log::error!("[OCW] Receipt verification failed {:?}", e);
								None
							}
						} else {
							log::error!("[OCW] Receipt verification failed {:?}", e);
							None
						},
				}
			},
			_ => None,
		};

		// Process result of the extrinsic
		if let Some(results) = results {
			for (acc, res) in &results {
				match res {
					Ok(()) => {
						log::info!("[OCW] [{:?}] Submitted tx", acc.id)
					},
					Err(e) => {
						log::error!(
							"[OCW] Failed to submit transaction from: [{:?}] reason: {:?}",
							acc.id,
							e
						);
						return Err("Failed to submit transaction")
					},
				}
			}
		}

		Ok(())
	}

	/// Parse bank statements
	///
	/// Returns:
	///
	/// - `iban_account: IbanAccount` - IBAN account that owns the statement
	/// - `incoming_txs: Vec<TransactionOf<T>>` - Incoming transactions in the statement
	/// - `outgoing_txs: Vec<TransactionOf<T>>` - Outgoing transactions in the statement
	fn parse_statements() -> Result<
		BTreeMap<
			StringOf<T>,
			(BankAccountOf<T>, BoundedVec<TransactionOf<T>, <T as Config>::MaxStatements>),
		>,
		&'static str,
	> {
		let json = Self::fetch_json("api-v1/bankstatements")?;

		let raw_array = json.as_array();

		if let Some(v) = raw_array {
			let mut balances = BTreeMap::new();

			for val in v.iter() {
				let as_object = val.as_object().ok_or("Error in parsing json")?;
				let receipt_url = extract_value::<Vec<u8>>("receiptUrl", as_object)
					.try_into()
					.unwrap_or_default();

				// extract iban account
				if let Ok(bank_account) = BankAccountOf::<T>::try_from(val) {
					// extract transactions
					let mut transactions =
						TransactionOf::<T>::parse_transactions(val, TransactionType::Outgoing)
							.unwrap_or_default();
					let mut incoming_transactions =
						TransactionOf::<T>::parse_transactions(val, TransactionType::Incoming)
							.unwrap_or_default();

					transactions.append(&mut incoming_transactions);

					// try pushing to balances
					balances.insert(
						receipt_url,
						(
							bank_account,
							transactions.try_into().expect("tx length should not exceed; qed"),
						),
					);
				}
			}
			Ok(balances)
		} else {
			Ok(BTreeMap::new())
		}
	}

	fn validate_tx_parameters() -> TransactionValidity {
		// check if we are on time
		if !Self::should_sync() {
			return InvalidTransaction::Future.into()
		}

		// check if the receipt is present

		let block_number = <frame_system::Pallet<T>>::block_number();

		ValidTransaction::with_tag_prefix("FiatRamps")
			.priority(T::UnsignedPriority::get())
			.and_provides(block_number)
			.longevity(64)
			.propagate(false)
			.build()
	}
}
