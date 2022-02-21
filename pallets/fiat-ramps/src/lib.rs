//! Fiat on-off ramps offchain worker
//! 
//! Polls Nexus API at a given interval to get the latest bank statement and 
//! updates the onchain state accordingly.

#![cfg_attr(not(feature = "std"), no_std)]
use codec::{Decode, Encode};
use scale_info::{TypeInfo, prelude::format};
use frame_support::{
	pallet_prelude::*,
	traits::{
		Get, UnixTime, Currency, LockableCurrency,
		ExistenceRequirement, WithdrawReasons, Imbalance
	},
	ensure, PalletId,
	dispatch::DispatchResultWithPostInfo
};
use frame_system::{
	pallet_prelude::*, ensure_signed,
	offchain::{
		SignedPayload, 
		SendSignedTransaction, 
		SigningTypes, 
		Signer,
		CreateSignedTransaction,
		AppCrypto
	},
};
use sp_runtime::{
	RuntimeDebug, offchain as rt_offchain,
	AccountId32, SaturatedConversion,
	transaction_validity::{
		InvalidTransaction, TransactionValidity,
	},
	traits::{
		AccountIdConversion
	},
	offchain::storage::{MutateStorageError, StorageRetrievalError, StorageValueRef}
};
use sp_core::{crypto::{KeyTypeId}};
use sp_std::prelude::{Vec};
use sp_runtime::{DispatchError};
use sp_std::{ convert::{TryFrom}, vec };

use lite_json::{
	json::{JsonValue}, 
	json_parser::{parse_json},
    NumberValue, Serialize,
};

use crate::types::{
	Transaction, IbanAccount, unpeg_request,
	TransactionType, StrVecBytes
};

#[cfg(feature = "std")]
use sp_core::{ crypto::Ss58Codec };

pub mod types;

#[cfg(test)]
mod tests;
/// Defines application identifier for crypto keys of this module.
///
/// Every module that deals with signatures needs to declare its unique identifier for
/// its crypto keys.
/// When an offchain worker is signing transactions it's going to request keys from type
/// `KeyTypeId` via the keystore to sign the transaction.
/// The keys can be inserted manually via RPC (see `author_insertKey`).
pub const KEY_TYPE: KeyTypeId = KeyTypeId(*b"ramp");

/// Pallet ID
/// Account id will be derived from this pallet id.
pub const PALLET_ID: PalletId = PalletId(*b"FiatRamp");

/// Account id of
pub type AccountIdOf<T> = <T as frame_system::Config>::AccountId;
/// Balance type
pub type BalanceOf<T> = <<T as Config>::Currency as Currency<AccountIdOf<T>>>::Balance;

/// Hardcoded inital test api endpoint
const API_URL: &[u8] = b"http://w.e36.io:8093/ebics/api-v1";

/// Based on the above `KeyTypeId` we need to generate a pallet-specific crypto type wrapper.
/// We can utilize the supported crypto kinds (`sr25519`, `ed25519` and `ecdsa`) and augment
/// them with the pallet-specific identifier.
pub mod crypto {
	use crate::KEY_TYPE;
	use sp_core::sr25519::Signature as Sr25519Signature;
	use sp_runtime::app_crypto::{app_crypto, sr25519};
	use sp_runtime::{traits::Verify, MultiSignature, MultiSigner};

	app_crypto!(sr25519, KEY_TYPE);

	pub struct OcwAuthId;

	impl frame_system::offchain::AppCrypto<MultiSigner, MultiSignature> for OcwAuthId {
		type RuntimeAppPublic = Public;
		type GenericSignature = sp_core::sr25519::Signature;
		type GenericPublic = sp_core::sr25519::Public;
	}

	// implemented for mock runtime in test
	impl frame_system::offchain::AppCrypto<<Sr25519Signature as Verify>::Signer, Sr25519Signature>
		for OcwAuthId
	{
		type RuntimeAppPublic = Public;
		type GenericSignature = sp_core::sr25519::Signature;
		type GenericPublic = sp_core::sr25519::Public;
	}
}

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	/// This is the pallet's trait
	#[pallet::config]
	pub trait Config: frame_system::Config + CreateSignedTransaction<Call<Self>> {
		/// The identifier type for an offchain SendSignedTransaction
		type AuthorityId: AppCrypto<Self::Public, Self::Signature>;
		/// The overarching dispatch call type.
		type Call: From<Call<Self>>;
		/// The overarching event type.
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

		/// Loosely coupled timestamp provider
		type TimeProvider: UnixTime;

		/// Currency type
		type Currency: Currency<Self::AccountId> + LockableCurrency<Self::AccountId, Moment = Self::BlockNumber>;

		// This ensures that we only accept unsigned transactions once, every `UnsignedInterval` blocks.
		#[pallet::constant]
		type MinimumInterval: Get<u64>;
		
		/// A configuration for base priority of unsigned transactions.
		///
		/// This is exposed so that it can be tuned for particular runtime, when
		/// multiple pallets send unsigned transactions.
		#[pallet::constant]
		type UnsignedPriority: Get<u64>;

		/// Decimals of the internal token
		#[pallet::constant]
		type Decimals: Get<u8>;
	}

	#[pallet::pallet]
	#[pallet::generate_store(pub(super) trait Store)]
	pub struct Pallet<T>(_);

	#[pallet::hooks]
	impl<T:Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn offchain_worker(block_number: T::BlockNumber) {
			log::info!("[OCW] Instantiating offchain worker");

			let parent_hash = <frame_system::Pallet<T>>::block_hash(block_number - 1u32.into());
			log::debug!("[OCW] Current block: {:?} (parent hash: {:?})", block_number, parent_hash);

			let should_sync = Self::should_sync();

			log::info!("[OCW] Syncing: {}", &should_sync);
			
			if !should_sync {
				log::error!("[OCW] Too early to sync");
				return ();
			}

			// Choose which activity to perform
			let activity = Self::choose_ocw_activity();

			log::info!("[OCW] Current activity: {:?}", &activity);

			let res = match activity {
				OcwActivity::ProcessStatements => {
					Self::fetch_transactions_and_send_signed()
				},
				OcwActivity::ProcessBurnRequests => {
					Self::process_burn_requests()
				},
				_ => {
					log::error!("[OCW] No activity to perform");
					Ok(())
				}
			};

			if let Err(e) = res {
				log::error!("[OCW] Error syncing with bank: {}", e);
			} else {
				let now = T::TimeProvider::now();
				log::info!("[OCW] Last sync timestamp: {}", now.as_millis());
			}
		}
	}

	#[pallet::call]
	impl<T:Config> Pallet<T> {
		/// Set api url for fetching bank statements
		// TO-DO change weight for appropriate value
		#[pallet::weight(0)]
		pub fn set_api_url(origin: OriginFor<T>, url: Vec<u8>) -> DispatchResultWithPostInfo {
			ensure_root(origin)?;
			<ApiUrl<T>>::put(url);
			Ok(().into())
		}

		/// Add new `IbanAccount` to the store
		/// 
		/// # Arguments
		/// 
		/// `iban`: IbanAccount struct
		#[pallet::weight(1000)]
		pub fn map_iban_account(
			origin: OriginFor<T>,
			iban: IbanAccount,
		) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;

			IbanToAccount::<T>::insert(iban.iban.clone(), who.clone());

			Self::deposit_event(Event::IbanAccountMapped(who, iban));

			Ok(().into())
		}

		/// Remove `IbanAccount` from the store
		/// 
		/// # Arguments
		/// 
		/// `iban`: IbanAccount struct
		#[pallet::weight(1000)]
		pub fn unmap_iban_account(
			origin: OriginFor<T>,
			iban: IbanAccount,
		) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;

			IbanToAccount::<T>::remove(iban.iban.clone());

			Self::deposit_event(Event::IbanAccountUnmapped(who, iban));

			Ok(().into())
		}

		/// Generic burn extrinsic
		/// 
		/// Creates new burn request in the pallet and sends `unpeq` request to remote endpoint
		/// 
		/// # Arguments
		/// 
		/// `amount`: Amount of tokens to burn
		#[pallet::weight(1000)]
		pub fn burn(
			origin: OriginFor<T>,
			amount: BalanceOf<T>,
		) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;

			// do basic validations
			let balance = T::Currency::free_balance(&who);

			ensure!(
				balance >= amount,
				"Not enough balance to burn",
			);

			// transfer amount to this pallet's account
			T::Currency::transfer(&who, &Self::account_id(), amount, ExistenceRequirement::AllowDeath)
				.map_err(|_| DispatchError::Other("Can't burn funds"))?;
			
			let iban = IbanToAccount::<T>::iter().find(|(_, v)| *v == who)
				.ok_or(DispatchError::Other("Can't find iban account"))?.0; 

			let request_id = Self::burn_request_count();

			let burn_request = BurnRequest {
				burner: who.clone(),
				dest: Some(IbanToAccount::<T>::get(&iban)),
				dest_iban: Some(iban.clone()),
				amount,
				status: BurnRequestStatus::Pending,
			};

			// Create new burn request in the storage
			<BurnRequests<T>>::insert(request_id, burn_request.clone());
			
			// Increase burn request count
			<BurnRequestCount<T>>::put(request_id + 1);

			// create burn request event
			Self::deposit_event(Event::BurnRequest(burn_request));

			Ok(().into())
		}

		/// Similar to `burn` but burns `amount` by sending it to `iban`
		/// 
		/// # Arguments
		/// 
		/// `amount`: Amount of tokens to burn
		/// `iban`: Iban account of the receiver
		#[pallet::weight(1000)]
		pub fn burn_to_iban(
			origin: OriginFor<T>,
			amount: BalanceOf<T>,
			iban: Vec<u8>,
		) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;
			
			let balance = T::Currency::free_balance(&who);

			ensure!(
				balance >= amount,
				"Not enough balance to burn",
			);

			// transfer amount to this pallet's account
			T::Currency::transfer(&who, &Self::account_id(), amount, ExistenceRequirement::AllowDeath)
				.map_err(|_| DispatchError::Other("Can't burn funds"))?;

			// Request id (nonce)
			let request_id = Self::burn_request_count();

			let burn_request = BurnRequest {
				burner: who.clone(),
				dest: Some(IbanToAccount::<T>::get(&iban)),
				dest_iban: Some(iban.clone()),
				amount,
				status: BurnRequestStatus::Pending,
			};

			// Create new burn request in the storage
			<BurnRequests<T>>::insert(request_id, burn_request.clone());

			// Increase burn request count
			<BurnRequestCount<T>>::put(request_id + 1);

			// create burn request event
			Self::deposit_event(Event::BurnRequest(burn_request));

			Ok(().into())
		}

		/// Similar to `burn` but burns `amount` by transfering it to `account` on-chain
		/// 
		/// # Arguments
		/// 
		/// `amount`: Amount of tokens to burn
		/// `account`: AccountId of the receiver
		#[pallet::weight(1000)]
		pub fn burn_to_address(
			origin: OriginFor<T>,
			amount: BalanceOf<T>,
			address: AccountIdOf<T>,
		) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;
			
			let balance = T::Currency::free_balance(&who);

			ensure!(
				balance >= amount,
				"Not enough balance to burn",
			);

			// transfer amount to this pallet's account
			T::Currency::transfer(&who, &Self::account_id(), amount, ExistenceRequirement::AllowDeath)
				.map_err(|_| DispatchError::Other("Can't burn funds"))?;
			
			let iban = IbanToAccount::<T>::iter().find(|(_, v)| *v == who)
				.ok_or(DispatchError::Other("Can't find iban account"))?.0; 

			// Request id (nonce)
			let request_id = Self::burn_request_count();

			let burn_request = BurnRequest {
				burner: who.clone(),
				dest: Some(address),
				dest_iban: Some(iban.clone()),
				amount,
				status: BurnRequestStatus::Pending,
			};

			// Create new burn request in the storage
			<BurnRequests<T>>::insert(request_id, burn_request.clone());
			
			// Increase burn request count
			<BurnRequestCount<T>>::put(request_id + 1);

			// create burn request event
			Self::deposit_event(Event::BurnRequest(burn_request));

			Ok(().into())
		}

		// TO-DO
		#[pallet::weight(1000)]
		pub fn mint(
			_origin: OriginFor<T>,
			_transaction: Transaction
		) -> DispatchResultWithPostInfo {
			// TO-DO
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
		#[pallet::weight(10_000)]
		pub fn process_statements(
			origin: OriginFor<T>,
			statements: Vec<(IbanAccount, Vec<Transaction>)>
		) -> DispatchResultWithPostInfo {
			// this can be called only by the sudo account
			let _who = ensure_signed(origin)?;

			// // TO-DO: need to make sure the signer is an OCW here
			// let signers = Signer::<T, T::AuthorityId>::all_accounts();

			// ensure!(
			// 	signers.contains(who.clone()),
			// 	"[OCW] Only OCW can call this function",
			// );

			log::info!("[OCW] Processing statements");
			
			for (iban_account, transactions) in statements {
				let should_process = Self::should_process_transactions(&iban_account);
				
				if should_process {
					#[cfg(feature = "std")]
					Self::process_transactions(&iban_account, &transactions);
				}
			}

			Ok(().into())
		}
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A new account has been created
		NewAccount(T::AccountId),
		/// New IBAN has been mapped to an account
		IbanAccountMapped(T::AccountId, IbanAccount),
		/// IBAN has been un-mapped from an account
		IbanAccountUnmapped(T::AccountId, IbanAccount),
		/// New minted tokens to an account
		Mint(T::AccountId, StrVecBytes, BalanceOf<T>),
		/// New burned tokens from an account
		Burn(T::AccountId, StrVecBytes, BalanceOf<T>),
		/// New Burn request has been made
		BurnRequest(BurnRequest<T::AccountId, BalanceOf<T>>),
		/// Transfer event with IBAN numbers
		Transfer(
			StrVecBytes, // from
			StrVecBytes, // to
			BalanceOf<T>
		)
	}

	#[pallet::validate_unsigned]
	impl<T: Config> ValidateUnsigned for Pallet<T> {
		type Call = Call<T>;
		fn validate_unsigned(_source: TransactionSource, call: &Self::Call) -> TransactionValidity {
			if let Call::process_statements { statements } = call {
				Self::validate_tx_parameters(statements)
			} else {
				InvalidTransaction::Call.into()
			}
		}
	}

	#[pallet::type_value]
	pub(super) fn DefaultBurnCount<T: Config>() -> u64 { 0 }

	/// Counts the number of burn requests, irrespective of the sender and burn request status
	#[pallet::storage]
	#[pallet::getter(fn burn_request_count)]
	pub(super) type BurnRequestCount<T: Config> = StorageValue<_, u64, ValueQuery, DefaultBurnCount<T>>;

	#[pallet::type_value]
	pub(super) fn DefaultApi<T: Config>() -> StrVecBytes { API_URL.iter().cloned().collect() }	

	/// URL of the remote API
	#[pallet::storage]
	#[pallet::getter(fn api_url)]
	pub(super) type ApiUrl<T: Config> = StorageValue<_, StrVecBytes, ValueQuery, DefaultApi<T>>;

	/// Mapping between IBAN to AccountId
	#[pallet::storage]
	#[pallet::getter(fn iban_to_account)]
	pub(super) type IbanToAccount<T: Config> = StorageMap<_, Blake2_128Concat, StrVecBytes, T::AccountId, ValueQuery>;

	/// Stores burn requests
	/// until they are confirmed by the bank as outgoing transaction
	/// transaction_id -> burn_request
	#[pallet::storage]
	#[pallet::getter(fn burn_request)]
	pub (super) type BurnRequests<T: Config> = StorageMap<_, Blake2_128Concat, u64, BurnRequest<T::AccountId, BalanceOf<T>>, ValueQuery>;
}

/// Status options for burn requests
#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
pub enum BurnRequestStatus {
	Pending,
	Sent,
	Failed,
	Confirmed,
}

impl Default for BurnRequestStatus {
	fn default() -> Self {
		BurnRequestStatus::Pending
	}
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
pub struct BurnRequest<Account: Default, Balance: Default> {
	pub burner: Account,
	pub dest: Option<Account>,
	pub dest_iban: Option<StrVecBytes>,
	pub amount: Balance,
	pub status: BurnRequestStatus,
}

impl<Account: Default, Balance: Default> Default for BurnRequest<Account, Balance> {
	fn default() -> Self {
		BurnRequest {
			burner: Default::default(),
			dest: None,
			dest_iban: None,
			amount: Default::default(),
			status: Default::default(),
		}
	}
}

/// Types of activities that can be performed by the OCW
#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
pub enum OcwActivity {
	ProcessStatements,
	ProcessBurnRequests,
	None,
}

impl Default for OcwActivity {
	fn default() -> Self {
		OcwActivity::None
	}
}

impl From<u32> for OcwActivity {
	fn from(activity: u32) -> Self {
		match activity {
			0 => OcwActivity::ProcessStatements,
			1 => OcwActivity::ProcessBurnRequests,
			_ => OcwActivity::None,
		}
	}
}

impl<T: Config> Pallet<T> {
	/// AccountId associated with Pallet
	fn account_id() -> T::AccountId {
		PALLET_ID.into_account()
	}

	/// Returns the action to take
	fn choose_ocw_activity() -> OcwActivity {
		// Get last activity recorded
		let last_activity_ref = StorageValueRef::persistent(b"fiat_ramps:last_activity");
		
		// Fetch last activity and set new one
		// Activity order is following: Process statements -> Process burn requests -> Do nothing -> Repeat
		let last_activity= last_activity_ref.mutate(|val: Result<Option<OcwActivity>, StorageRetrievalError>| {
			match val {
				Ok(Some(activity)) => {
					match activity {
						OcwActivity::ProcessStatements => Ok(OcwActivity::ProcessBurnRequests),
						OcwActivity::ProcessBurnRequests => Ok(OcwActivity::None),
						OcwActivity::None => Ok(OcwActivity::ProcessStatements),
					}
				}
				Ok(None) => Ok(OcwActivity::ProcessStatements),
				_ => Ok(OcwActivity::None),
			}
		});

		match last_activity {
			Ok(activity) => activity,
			Err(MutateStorageError::ValueFunctionFailed(())) => OcwActivity::ProcessStatements,
			Err(MutateStorageError::ConcurrentModification(_)) => OcwActivity::None
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
				Ok(Some(last_sync_at)) if now.as_millis() < last_sync_at + minimum_interval as u128 =>
					Err(RECENTLY_SENT),
				// In every other case we attempt to acquire the lock and send a transaction.
				_ => Ok(now.as_millis()),
			}
		});

		match res {
			Ok(_now) => true,
			Err(MutateStorageError::ValueFunctionFailed(RECENTLY_SENT)) => false,
			Err(MutateStorageError::ConcurrentModification(_)) => false
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
	fn should_process_transactions(iban_account: &IbanAccount) -> bool {
		// if iban is not registered in our store, we should process transactions
		if !Self::iban_exists(iban_account.iban.clone()) {
			return true;
		}

		let account_id = Self::iban_to_account(iban_account.iban.clone());

		// balance of the iban account on chain
		let _on_chain_balance = T::Currency::free_balance(&account_id);

		// TODO: uncomment this when we are sure
		// // sync transactions if balances on chain and on the statement do not match
		// if on_chain_balance.saturated_into::<u128>() != iban_account.balance {
		// 	return true;
		// }

		return true;
	}

	/// Checks if iban is mapped to an account in the storage
	fn iban_exists(iban: StrVecBytes) -> bool {
		IbanToAccount::<T>::contains_key(iban)
	}

	/// Ensures that an IBAN  is mapped to an account in the storage
	/// 
	/// If necessary, creates new account
	#[cfg(feature = "std")]
	fn ensure_iban_is_mapped(
		iban: StrVecBytes, 
		account: Option<AccountIdOf<T>>
	) -> AccountIdOf<T> {
		// If iban is already mapped to account, return it
		if Self::iban_exists(iban.clone()) {
			return Self::iban_to_account(iban);
		}
		else {
			match account {
				Some(account_id) => {
					// Simply map iban to account
					IbanToAccount::<T>::insert(iban.clone(), account_id.clone());
					return account_id;
				}
				None => {
					// Generate new keypair
					let (pair, _, _) = <crypto::Pair as sp_core::Pair>::generate_with_phrase(None);
					
					// Convert AccountId32 to AccountId
					let encoded = sp_core::Pair::public(&pair).encode();
					let new_account_id = <T::AccountId>::decode(&mut &encoded[..]).unwrap();

					// Map new account id to IBAN
					IbanToAccount::<T>::insert(iban.clone(), new_account_id.clone());

					return new_account_id;
				}
			}
		}
	}

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
		statement_iban: &StrVecBytes,
		source: Option<T::AccountId>,
		dest: Option<T::AccountId>,
		transaction: &Transaction,
		reference: Option<u64>,
	) {
		let amount: BalanceOf<T> = BalanceOf::<T>::try_from(transaction.amount).unwrap_or_default();

		// Process transaction based on its type
		match transaction.tx_type {
			TransactionType::Incoming => {
				match source {
					Some(sender) => {
						log::info!("[OCW] Transfer from {:?} to {:?} {:?}", sender, statement_owner, amount.clone());
						
						// make transfer from sender to statement owner
						match <T>::Currency::transfer(
							&sender,
							statement_owner, 
							amount, 
							ExistenceRequirement::AllowDeath
						) {
							Ok(_) => {
								Self::deposit_event(
									Event::Transfer(
										transaction.iban.clone(), // from iban
										statement_iban.clone(), // to iban
										amount
									)
								)
							},
							Err(e) => {
								log::error!(
									"[OCW] Transfer from {:?} to {:?} {:?} failed: {:?}", 
									&sender, 
									statement_owner, 
									amount.clone(), 
									e
								);
							}
						}
					},
					None => {
						// Sender is not on-chain, therefore we simply mint to statement owner
						log::info!("[OCW] Mint to {:?} {:?}", statement_owner, amount.clone());

						// Returns negative imbalance
						let mint = <T>::Currency::issue(
							amount.clone()
						);
		
						// deposit negative imbalance into the account
						<T>::Currency::resolve_creating(
							statement_owner,
							mint
						);
		
						Self::deposit_event(Event::Mint(statement_owner.clone(), statement_iban.clone(), amount));
					}
				}
			}
			TransactionType::Outgoing => {
				match dest {
					Some(receiver) => {
						log::info!("[OCW] Transfer from {:?} to {:?} {:?}", statement_owner, receiver, amount.clone());

						let burn_request = match reference {
							Some(request_id) => {
								match BurnRequests::<T>::try_get(request_id) {
									Ok(request) => {
										BurnRequests::<T>::mutate(request_id, |v| {
											*v = BurnRequest {
												status: BurnRequestStatus::Confirmed,
												..v.clone()
											}
										});
										Some(request)
									},
									_ => None
								}
							},
							None => None
						};

						// Here we get the actual sender and receiver of the transaction
						// If user has submitted burn request, his funds are stored in the pallet's account
						// and if we detect that the reference field is populated with a burn request id,
						// we can transfer the funds from the pallet's account to the specified destination
						// account specified in the burn request.
						//
						// Otherwise, we simply transfer the funds from the statement owner to the receiver
						let (from, to) = match burn_request {
							Some(request) => (Self::account_id(), request.dest.unwrap_or(receiver.clone())),
							None => (statement_owner.clone(), receiver.clone())
						};
						
						// make transfer from statement owner to receiver
						match <T>::Currency::transfer(
							&from,
							&to,
							amount,
							ExistenceRequirement::AllowDeath
						) {
							Ok(_) => {
								Self::deposit_event(
									Event::Transfer(
										statement_iban.clone(), // from iban
										transaction.iban.clone(), // to iban
										amount
									)
								)
							},
							Err(e) => {
								log::error!("[OCW] Transfer from {:?} to {:?} {:?} failed: {:?}", statement_owner, receiver, amount.clone(), e);
							}
						}
					},
					None => {
						// Receiver is not on-chain, therefore we simply burn from statement owner
						log::info!("[OCW] Burn from {:?} {:?}", statement_owner, amount.clone());

						// Returns negative imbalance
						let burn = <T>::Currency::burn(
							amount.clone()
						);
		
						// Burn negative imbalance from the account
						let settle_burn = <T>::Currency::settle(
							statement_owner,
							burn,
							WithdrawReasons::TRANSFER,
							ExistenceRequirement::KeepAlive
						);
						
						match settle_burn {
							Ok(()) => {
								Self::deposit_event(Event::Burn(statement_owner.clone(), transaction.iban.clone(), amount));
								
								match reference {
									Some(request_id) => {
										BurnRequests::<T>::mutate(request_id, |v| {
											*v = BurnRequest {
												status: BurnRequestStatus::Confirmed,
												..v.clone()
											}
										});
										
										let request = BurnRequests::<T>::get(request_id);

										Self::deposit_event(Event::BurnRequest(request.clone()));

										// Returns negative imbalance
										let burn = <T>::Currency::burn(
											request.amount
										);
						
										// Burn negative imbalance from the account
										match <T>::Currency::settle(
											&Self::account_id(),
											burn,
											WithdrawReasons::TRANSFER,
											ExistenceRequirement::KeepAlive
										) {
											Ok(()) => log::info!("[OCW] Burn from pallet {:?} {:?}", Self::account_id(), request.amount),
											_ => log::info!("[OCW] Failed to burn from account {:?} {:?}", Self::account_id(), request.amount)
										}
									},
									None => {}
								}
							},
							Err(e) => log::error!("[OCW] Encountered err: {:?}", e.peek()),
						}
					}
				}
			}
			_ => {
				log::error!("[OCW] Unknown transaction type: {:?}", transaction.tx_type);
			}
		}
	}

	/// Process list of transactions for a given iban account
	/// 
	/// # Arguments
	/// 
	/// `iban: IbanAccount` - iban account to process transactions for
	/// `transactions: Vec<Transaction>` - list of transactions to process
	#[cfg(feature = "std")]
	fn process_transactions(
		iban: &IbanAccount, 
		transactions: &Vec<Transaction>
	) {
		// Get account id of the statement owner
		let statement_owner = Self::ensure_iban_is_mapped(iban.iban.clone(), None);

		for transaction in transactions {
			// decode destination account id from reference
			let reference_str = core::str::from_utf8(&transaction.reference).unwrap_or("default");

			// Format of the reference is the following:
			// Purpose:AccountId; ourReference:nonce(of burn request) 
			// E.g, "Purp:5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty; ourRef:12",
			let reference_decoded: Vec<&str> = reference_str.split(";").collect();

			log::info!("[OCW] Purpose: {}", reference_decoded[0]);
			log::info!("[OCW] Reference: {}", reference_decoded[1]);

			// Source (initiator) of the transaction
			let source: Option<AccountIdOf<T>> = match transaction.tx_type {
				TransactionType::Incoming => {
					if Self::iban_exists(transaction.iban.clone()) {
						Some(IbanToAccount::<T>::get(transaction.iban.clone()).into())
					} else {
						None
					}
				}
				TransactionType::Outgoing => {
					if Self::iban_exists(iban.iban.clone()) {
						Some(IbanToAccount::<T>::get(iban.clone().iban).into())
					} else {
						None
					}
				}
				_ => None
			};

			// Destination (recipient) of the transaction
			let dest = match AccountId32::from_ss58check(&reference_decoded[0][5..]) {
				Ok(dest) => Some(<T::AccountId>::decode(&mut &dest.encode()[..]).unwrap()),
				Err(_e) => {
					log::error!("[OCW] Failed to decode destination account from reference");
					match transaction.tx_type {
						TransactionType::Incoming => {
							if Self::iban_exists(iban.iban.clone()) {
								Some(IbanToAccount::<T>::get(iban.iban.clone()).into())
							}
							else {
								None
							}
						}
						TransactionType::Outgoing => {
							if Self::iban_exists(transaction.iban.clone()) {
								Some(IbanToAccount::<T>::get(transaction.iban.clone()).into())
							}
							else {
								None
							}
						}
						_ => None
					}
				}
			};

			// Proces transaction based on the value of reference
			// If decoding returns error, we look for the iban in the pallet storage
			Self::process_transaction(
				&statement_owner,
				&iban.iban,
				source, 
				dest, 
				transaction, 
				reference_decoded[1][7..].split_whitespace().collect::<String>().parse::<u64>().ok()
			);
		}
	}

	/// Send unpeq request to the remote endpoint
	/// Populates the unpeg request and sends it
	///
	/// Note: This function is not called from the runtime, but from the OCW module
	/// 
	/// ### Arguments
	/// 
	/// * `account_id` - AccountId of the burner, used to populate `purpose` field
	/// * `to_iban` - IBAN destination
	/// * `amount` - Amount to be burned
	fn unpeg(
		request_id: u64,
		dest: Option<AccountIdOf<T>>,
		to_iban: Option<StrVecBytes>, 
		amount: BalanceOf<T>
	) -> Result<(), &'static str> {
		let remote_url = ApiUrl::<T>::get();
		
		let remote_url_str = core::str::from_utf8(&remote_url[..])
			.map_err(|_| "Error in converting remote_url to string")?;
		
		// add /unpeg to the url
		let remote_url_str = format!("{}/unpeg", remote_url_str);

		let amount_u128 = amount.saturated_into::<u128>();

		// In the reference field, we save the request id (nonce)
		let reference = format!("{}", request_id);

		// Send request to remote endpoint
		let body = unpeg_request(
				&format!("{:?}", dest.unwrap_or(Self::account_id())),
				amount_u128,
				&to_iban.unwrap_or(b"Withdraw cash".to_vec()),
				&reference
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

		let response = post_request.wait()
			.map_err(|_| "Error in waiting http response back")?;

		log::info!("[OCW] Unpeg response received {:?}", response.code);
		
		if response.code != 200 {
			return Err("Error in unpeg response");
		}

		Ok(())
	}

	/// Process burn requets 
	///
	/// Processes registered burn requests, by sending http call to `unpeg` endpoint
	fn process_burn_requests() -> Result<(), &'static str> {
		for (request_id, burn_request) in <BurnRequests<T>>::iter() {
			// Process burn requests that are either not processed yet or failed
			if burn_request.status == BurnRequestStatus::Pending || burn_request.status == BurnRequestStatus::Failed {

				// Send unpeg request
				match Self::unpeg(
					request_id, 
					burn_request.dest,
					burn_request.dest_iban, 
					burn_request.amount
				) {
					Ok(_) => {
						log::info!("[OCW] Unpeq request successfull");
						BurnRequests::<T>::mutate(
							request_id, 
							|v| *v = BurnRequest {
								status: BurnRequestStatus::Sent,
								..v.clone()
							}
						);
					},
					Err(e) => {
						log::info!("[OCW] Unpeq request failed {}", e);
						BurnRequests::<T>::mutate(
							request_id, 
							|v|*v = BurnRequest {
								status: BurnRequestStatus::Failed,
								..v.clone()
							}
						);
					}
				};
			}
		}

		Ok(())
	}

	/// Fetch json from the Ebics Service API
	/// Return parsed json file
	fn fetch_json<'a>(remote_url: &'a [u8]) -> Result<JsonValue, &str> {
		let remote_url_str = core::str::from_utf8(remote_url)
			.map_err(|_| "Error in converting remote_url to string")?;

		let remote_url = format!("{}/bankstatements", remote_url_str);

		let pending = rt_offchain::http::Request::get(&remote_url).send()
			.map_err(|_| "Error in sending http GET request")?;

		let response = pending.wait()
			.map_err(|_| "Error in waiting http response back")?;

		if response.code != 200 {
			// runtime_print!("Unexpected status code: {}", response.code);
			return Ok(JsonValue::Null)
		}

		let json_result: Vec<u8> = response.body().collect::<Vec<u8>>();
		
		let json_str: &str = match core::str::from_utf8(&json_result) {
			Ok(v) => v,
			Err(_e) => "Error parsing json"
		};

		let json_val = parse_json(json_str).expect("Invalid json");
		// runtime_print!("json_val {:?}", json_val);
		Ok(json_val)
	}

	/// Fetch transactions from ebics service
	/// Parse the json and return a vector of statements
	/// Process the statements
	fn fetch_transactions_and_send_signed() -> Result<(), &'static str> {
		log::info!("[OCW] Fetching statements");

		// get extrinsic signer
		let signer = Signer::<T, T::AuthorityId>::all_accounts();
		
		if !signer.can_sign() {
			return Err("No local accounts available! Please, insert your keys!")
		}

		// Get statements from remote endpoint
		let statements= Self::parse_statements();
		
		// If statements are empty, do nothing
		if statements.is_empty() {
			return Ok(())
		}

		let results = signer.send_signed_transaction(|_account| {
			Call::process_statements { statements: statements.clone() }
		});

		// Process result of the extrinsic
		for (acc, res) in &results {
			match res {
				Ok(()) => {
					log::info!("[OCW] [{:?}] Submitted minting", acc.id)
				},
				Err(e) => log::error!("[OCW] [{:?}] Failed to submit transaction: {:?}", acc.id, e),
			}
		}

		Ok(())
	}

	/// parse bank statement
	///
	/// returns:
	/// 
	/// * `statements` - vector of statements
	/// 	`iban_account: IbanAccount` - IBAN account that owns the statement
	/// 	`incoming_txs: Vec<Transacion>` - Incoming transactions in the statement
    ///		`outgoing_txs: Vec<Transaction>` - Outgoing transactions in the statement
	fn parse_statements() -> Vec<(IbanAccount, Vec<Transaction>)> {
		// fetch json value
		let remote_url = ApiUrl::<T>::get();
		let json = Self::fetch_json(&remote_url[..]).unwrap();

		let raw_array = json.as_array();

		let statements = match raw_array {
			Some(v) => {
				let mut balances: Vec<(IbanAccount, Vec<Transaction>)> = Vec::with_capacity(v.len());
				for val in v.iter() {
					// extract iban account
					let iban_account = match IbanAccount::from_json_value(&val) {
						Some(account) => account,
						None => Default::default(),
					};

					// extract transactions
					let mut transactions = Transaction::parse_transactions(&val, TransactionType::Outgoing).unwrap_or_default();
					let mut incoming_transactions = Transaction::parse_transactions(&val, TransactionType::Incoming).unwrap_or_default();
					
					transactions.append(&mut incoming_transactions);
					
					balances.push((iban_account, transactions));
				}
				balances
			},
			None => Default::default(),
		};
		statements
	}

	fn validate_tx_parameters(
		statements: &Vec<(IbanAccount, Vec<Transaction>)>
	) -> TransactionValidity {
		// check if we are on time
		if !Self::should_sync() {
			return InvalidTransaction::Future.into()
		}

		let block_number = <frame_system::Pallet<T>>::block_number();

		ValidTransaction::with_tag_prefix("FiatRamps")
			.priority(T::UnsignedPriority::get().saturating_add(statements.capacity() as u64))
			.and_provides(block_number)
			.longevity(64)
			.propagate(true)
			.build()
	}
}
