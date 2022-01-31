//! A demonstration of an offchain worker that sends onchain callbacks

#![cfg_attr(not(feature = "std"), no_std)]
use codec::{Decode, Encode};
use frame_support::{
	pallet_prelude::{ValidTransaction},
	traits::{
		Get, UnixTime, Currency, LockableCurrency,
		ExistenceRequirement, WithdrawReasons
	},
	PalletId,
};
use frame_system::offchain::SendSignedTransaction;
use lite_json::{NumberValue, Serialize};
use lite_json::{json::{JsonValue}, json_parser::{parse_json}};
use frame_system::{offchain::{AppCrypto, CreateSignedTransaction, SignedPayload, SigningTypes, Signer}};
use sp_core::{crypto::{KeyTypeId}};
use sp_runtime::offchain::storage::{MutateStorageError, StorageRetrievalError, StorageValueRef};
use sp_runtime::{
	RuntimeDebug, offchain as rt_offchain,
	AccountId32, SaturatedConversion,
	transaction_validity::{
		InvalidTransaction, TransactionValidity,
	},
	traits::{
		AccountIdConversion
	}
};
use sp_std::prelude::{Vec};
use scale_info::prelude::format;

#[cfg(not(feature = "std"))]
use sp_std::prelude::vec;
use sp_std::convert::{TryFrom};

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
const API_URL: &[u8] = b"https://61439649c5b553001717d029.mockapi.io/statements";

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
	use frame_support::{pallet_prelude::*, traits::{UnixTime}, ensure, dispatch::DispatchResultWithPostInfo};
	use frame_system::{pallet_prelude::*, ensure_signed};
	use sp_runtime::{DispatchError};

	/// This is the pallet's trait
	#[pallet::config]
	pub trait Config: frame_system::Config + CreateSignedTransaction<Call<Self>> {
		/// The identifier type for an offchain worker.
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
		type UnsignedPriority: Get<TransactionPriority>;

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
			let res = Self::fetch_transactions_and_send_signed();
			// let res = Self::fetch_iban_balance_and_send_unsigned(block_number);

			if let Err(e) = res {
				log::error!("[OCW] Error: {}", e);
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

			AccountToIban::<T>::insert(who.clone(), iban.clone());

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

			AccountToIban::<T>::remove(who.clone());

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
			
			let iban_account = AccountToIban::<T>::get(&who);

			// actions related to burn
			#[cfg(feature = "std")]
			match Self::unpeg(who, iban_account.iban, amount) {
				Ok(_) => {
					log::info!("[OCW] Burn successful");
				},
				Err(e) => {
					log::error!("[OCW] Burn failed: {}", e);
				}
			}

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
			
			// actipns related to burn
			match Self::unpeg(who, iban, amount) {
				Ok(_) => {
					log::info!("[OCW] Burn to iban successful");
				},
				Err(e) => {
					log::error!("[OCW] Burn to iban failed: {}", e);
				}
			}

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
			
			let iban_account = AccountToIban::<T>::get(&who);

			// actions related to burn
			match Self::unpeg(address, iban_account.iban, amount) {
				Ok(_) => {
					log::info!("[OCW] Burn to address successful");
				},
				Err(e) => {
					log::error!("[OCW] Burn to address failed: {}", e);
				}
			}

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
			let who = ensure_signed(origin)?;

			log::info!("[OCW] Processing statments");

			ensure!(
				who == Self::offchain_worker_account(),
				"No statements to process",
			);
			
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
		NewAccount(T::AccountId),
		IbanAccountMapped(T::AccountId, IbanAccount),
		IbanAccountUnmapped(T::AccountId, IbanAccount),
		Mint(T::AccountId, StrVecBytes, BalanceOf<T>),
		Burn(T::AccountId, StrVecBytes, BalanceOf<T>),
		BurnRequest(u64, T::AccountId, BalanceOf<T>),
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
			if let Call::process_statements(statements) = call {
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

	/// Mapping between AccountId and IbanAccount
	#[pallet::storage]
	#[pallet::getter(fn account_to_iban)]
	pub(super) type AccountToIban<T: Config> = StorageMap<_, Blake2_128Concat, T::AccountId, IbanAccount, ValueQuery>;

	/// Stores burn requests
	/// until they are confirmed by the bank as outgoing transaction
	/// transaction_id -> burn_request
	#[pallet::storage]
	#[pallet::getter(fn burn_request)]
	pub (super) type BurnRequests<T: Config> = StorageMap<_, Blake2_128Concat, u64, (T::AccountId, BalanceOf<T>, BurnRequestStatus), ValueQuery>;
}


#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug)]
pub enum BurnRequestStatus {
	Pending,
	Failed,
	Confirmed,
}

impl Default for BurnRequestStatus {
	fn default() -> Self {
		BurnRequestStatus::Pending
	}
}

impl<T: Config> Pallet<T> {
	/// AccountId associated with Pallet
	fn account_id() -> T::AccountId {
		PALLET_ID.into_account()
	}

	/// checks whether we should sync in the current timestamp
	fn should_sync() -> bool {
		/// A friendlier name for the error that is going to be returned in case we are in the grace
		/// period.
		const RECENTLY_SENT: () = ();

		let now = T::TimeProvider::now();
		let minimum_interval = T::MinimumInterval::get();


		// Start off by creating a reference to Local Storage value.
		// Since the local storage is common for all offchain workers, it's a good practice
		// to prepend your entry with the module name.
		let val = StorageValueRef::persistent(b"fiat_ramps::last_sync");
		// The Local Storage is persisted and shared between runs of the offchain workers,
		// and offchain workers may run concurrently. We can use the `mutate` function, to
		// write a storage entry in an atomic fashion. Under the hood it uses `compare_and_set`
		// low-level method of local storage API, which means that only one worker
		// will be able to "acquire a lock" and send a transaction if multiple workers
		// happen to be executed concurrently.
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
		let on_chain_balance = T::Currency::free_balance(&account_id);

		// sync transactions if balances on chain and on the statement do not match
		if on_chain_balance.saturated_into::<u128>() != iban_account.balance {
			return true;
		}

		return false;
	}

	/// Check if iban exists in the storage
	fn iban_exists(iban: StrVecBytes) -> bool {
		IbanToAccount::<T>::contains_key(iban)
	}

	/// Process a single transaction
	/// 
	/// # Arguments
	/// 
	/// - `account_id`: `AccountId mapped to the `iban`
	/// - `iban`: This is the IBAN account of the user whose statement is being processed
	/// - `transaction`: The transaction to be processed, can be either `Incoming` or `Outgoing`
	fn process_transaction(
		account_id: &T::AccountId,
		iban: &StrVecBytes,
		transaction: &Transaction,
		reference: &str,
	) {
		let amount: BalanceOf<T> = BalanceOf::<T>::try_from(transaction.amount).unwrap_or_default();

		match transaction.tx_type {
			TransactionType::Incoming => {
				// transaction iban field is the sender of the transaction
				// we check if the iban is stored in our account,
				// if it is here, we transfer the amount from the sender to the account_id (receiver)
				if Self::iban_exists(transaction.iban.clone()) {
					let sender = Self::iban_to_account(transaction.iban.clone());
					
					log::info!("[OCW] Transfer from {:?} to {:?} {:?}", sender, account_id, amount.clone());

					// make transfer from sender to receiver
					match <T>::Currency::transfer(
						&sender,
						&account_id, 
						amount, 
						ExistenceRequirement::AllowDeath
					) {
						Ok(_) => {
							Self::deposit_event(
								Event::Transfer(
									iban.to_vec(), // from iban
									transaction.iban.clone(), // to iban
									amount
								)
							)
						},
						Err(e) => {
							log::error!("[OCW] Transfer from {:?} to {:?} {:?} failed: {:?}", account_id, sender, amount.clone(), e);
						}
					}
				}
				// if the iban i.e sender is not on-chain, 
				// we issue transaction amount to the account_id. 
				// It is similar to minting.
				else {
					log::info!("[OCW] Mint {:?} to {:?}", &amount, &account_id);
					
					// mint tokens, returns a negative imbalance
					let mint = <T>::Currency::issue(
						amount.clone()
					);
	
					// deposit negative imbalance into the account
					<T>::Currency::resolve_creating(
						&account_id,
						mint
					);
	
					Self::deposit_event(Event::Mint(account_id.clone(), transaction.iban.clone(), amount));
				}
			},
			// In this case, transaction iban field is the receiver of the transaction
			// we first check if the iban is stored in the storage
			TransactionType::Outgoing => {
				// If receiver address of the transaction exists in our storage, 
				// we transfer the amount
				if Self::iban_exists(transaction.iban.clone()) {
					log::info!("transfering: {:?} to {:?}", &amount, &account_id);

					let dest: T::AccountId = IbanToAccount::<T>::get(transaction.iban.clone()).into();
					
					// perform transfer
					match <T as pallet::Config>::Currency::transfer(
						account_id, 
						&dest, 
						amount.clone(),
						ExistenceRequirement::KeepAlive
					) {
						Ok(()) => {
							Self::deposit_event(
								Event::Transfer(
									iban.to_vec(), // from iban
									transaction.iban.clone(), // to iban
									amount
								)
							)
						},
						Err(e) => log::error!("[OCW] Encountered err: {:?}", e),
					}
				}
				else {
					// We burn the amount here and settle the balance of the user
					log::info!("[OCW] Burn: {:?} to {:?}", &amount, &account_id);

					let res = <T>::Currency::burn(amount);
					
					let settle_res = <T>::Currency::settle(
						account_id,
						res,
						WithdrawReasons::TRANSFER,
						ExistenceRequirement::KeepAlive
					);

					match settle_res {
						Ok(()) => Self::deposit_event(Event::Burn(account_id.clone(), transaction.iban.clone(), amount)),
						Err(_e) => log::error!("[OCW] Encountered err burning"),
					}
				}
			},
			_ => log::info!("[OCW] Transaction type not supported yet!")
		};
	}

	/// Process list of transactions for a given iban account
	/// 
	/// # Arguments
	/// 
	/// `iban: IbanAccount` - iban account to process transactions for
	/// `transactions: Vec<Transaction>` - list of transactions to process
	#[cfg(feature = "std")]
	fn process_transactions(iban: &IbanAccount, transactions: &Vec<Transaction>) {
		for transaction in transactions {
			// decode destination account id from reference
			let reference_str = core::str::from_utf8(&transaction.reference).unwrap_or("default");

			// Format of the reference is the following:
			// Purpose:AccountId; ourReference:nonce(of burn request) 
			// E.g, "Purp:5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty; ourRef:12",
			let reference_decoded: Vec<&str> = reference_str.split(";").collect();

			// proces transaction based on the value of reference
			// if decoding returns error, we look for the iban in the pallet storage
			match AccountId32::from_ss58check(&reference_decoded[0][5..].to_string()) {
				Ok(account_id) => {
					let encoded = account_id.encode();
					let account = <T::AccountId>::decode(&mut &encoded[..]).unwrap();
					Self::process_transaction(
						&account,
						&iban.iban,
						transaction,
						reference_decoded[1][9..].to_string()
					);
					<IbanToAccount<T>>::insert(iban.iban.clone(), account);
				},
				Err(_e) => {
					// if iban exists in our storage, get accountId from there
					// this would be initiator of the transaction
					if Self::iban_exists(iban.iban.clone()) {
						let connected_account_id: T::AccountId = IbanToAccount::<T>::get(iban.iban.clone()).into();
						Self::process_transaction(
							&connected_account_id,
							&iban.iban,
							transaction,
							reference_decoded[1][9..].to_string()
						);
					}

					// if no iban mapping exists in the storage, create new AccountId for Iban
					else {
						let (pair, _, _) = <crypto::Pair as sp_core::Pair>::generate_with_phrase(None);
						let encoded = sp_core::Pair::public(&pair).encode();
						let account_id = <T::AccountId>::decode(&mut &encoded[..]).unwrap();
						
						log::info!("[OCW] Create new account: {:?}", &account_id);

						Self::process_transaction(
							&account_id, 
							&iban.iban,
							transaction,
							reference_decoded[1][9..].to_string()
						);
						Self::deposit_event(Event::NewAccount(account_id.clone()));

						<IbanToAccount<T>>::insert(iban.iban.clone(), account_id);
					}
				}
			}
		}
	}

	/// Perform actions related to burn request
	fn create_burn_request(burner: &T::AccountId, amount: BalanceOf<T>, request_id: u64) {
		// record burn request
		<BurnRequests<T>>::insert(request_id, (
			burner.clone(),
			amount,
			BurnRequestStatus::Pending
		));

		// create burn request event
		Self::deposit_event(Event::BurnRequest(request_id, burner.clone(), amount));
	}

	/// Send unpeq request to the remote endpoint
	/// Populates the unpeg request and sends it
	///
	/// # Arguments
	/// 
	/// * `account_id` - AccountId of the burner, used to populate `purpose` field
	/// * `to_iban` - IBAN destination
	/// * `amount` - Amount to be burned
	fn unpeg(
		account_id: AccountIdOf<T>,
		to_iban: StrVecBytes, 
		amount: BalanceOf<T>
	) -> Result<(), &'static str> {
		let remote_url = ApiUrl::<T>::get();
		
		let remote_url_str = core::str::from_utf8(&remote_url[..])
			.map_err(|_| "Error in converting remote_url to string")?;
		
		// add /unpeg to the url
		let remote_url_str = format!("{}/unpeg", remote_url_str);

		let amount_u128 = amount.saturated_into::<u128>();

		// in the reference field, we save the nonce of the user
		let reference = format!("{}", Self::burn_request_count());

		// Send request to remote endpoint
		let body = unpeg_request(
				&format!("{:?}", account_id),
				amount_u128,
				&to_iban,
				&reference
			)
			.serialize();

		let pending = rt_offchain::http::Request::post(
			&remote_url_str,
			vec![body]
			)
			.send()
			.map_err(|_| "Error in sending http POST request")?;

		let response = pending.wait()
			.map_err(|_| "Error in waiting http response back")?;
		
		if response.code != 200 {
			return Err("Error in unpeg response");
		}

		// do unpeg related actions
		Self::create_burn_request(&account_id, amount, Self::burn_request_count());

		// increment burn request count
		<BurnRequestCount<T>>::mutate(|c| *c += 1);

		Ok(())
	}

	/// Fetch json from the Ebics Service API
	/// Return parsed json file
	fn fetch_json<'a>(remote_url: &'a [u8]) -> Result<JsonValue, &str> {
		let remote_url_str = core::str::from_utf8(remote_url)
			.map_err(|_| "Error in converting remote_url to string")?;

		let pending = rt_offchain::http::Request::get(remote_url_str).send()
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
			Call::process_statements(statements.clone())
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

impl<T: Config> rt_offchain::storage_lock::BlockNumberProvider for Pallet<T> {
	type BlockNumber = T::BlockNumber;
	fn current_block_number() -> Self::BlockNumber {
		<frame_system::Pallet<T>>::block_number()
	}
}
