//! A demonstration of an offchain worker that sends onchain callbacks

#![cfg_attr(not(feature = "std"), no_std)]
use codec::{Decode, Encode};
use frame_support::traits::{ExistenceRequirement, WithdrawReasons, fungible};
use frame_support::{pallet_prelude::ValidTransaction};
use frame_support::traits::{Get, UnixTime, Currency, LockableCurrency, tokens::{ fungible::{ Mutate } }};
use frame_system::offchain::SendSignedTransaction;
use lite_json::{json::{JsonValue}, json_parser::{parse_json}};
use frame_system::{offchain::{AppCrypto, CreateSignedTransaction, SignedPayload, SigningTypes, Signer}};
use sp_core::{crypto::{KeyTypeId}};
use sp_runtime::AccountId32;
use sp_runtime::offchain::storage::{MutateStorageError, StorageRetrievalError, StorageValueRef};
use sp_runtime::{RuntimeDebug, offchain as rt_offchain, transaction_validity::{
		InvalidTransaction, TransactionValidity
	}};
use sp_std::vec::Vec;
use sp_std::convert::TryFrom;

#[cfg(feature = "std")]
use sp_core::crypto::Ss58Codec;

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
	// implemented for ocw-runtime
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
	use frame_support::{pallet_prelude::*, traits::{UnixTime}};
	use frame_system::pallet_prelude::*;

	/// This is the pallet's configuration trait
	#[pallet::config]
	pub trait Config: pallet_timestamp::Config + frame_system::Config + CreateSignedTransaction<Call<Self>> {
		/// The identifier type for an offchain worker.
		type AuthorityId: AppCrypto<Self::Public, Self::Signature>;
		/// The overarching dispatch call type.
		type Call: From<Call<Self>>;
		/// The overarching event type.
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

		/// Loosely coupled timestamp provider
		type TimeProvider: UnixTime;

		/// Currency type
		type Currency: Currency<Self::AccountId> + LockableCurrency<Self::AccountId, Moment = Self::BlockNumber> + Mutate<Self::AccountId>;

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
			log::info!("Instantiating offchain worker");

			let parent_hash = <frame_system::Pallet<T>>::block_hash(block_number - 1u32.into());
			log::debug!("Current block: {:?} (parent hash: {:?})", block_number, parent_hash);

			let should_sync = Self::should_sync();

			log::info!("should sync: {}", &should_sync);
			
			if !should_sync {
				log::info!("Too early to sync");
				return ();
			}
			let res = Self::fetch_transactions_and_send_signed();
			// let res = Self::fetch_iban_balance_and_send_unsigned(block_number);

			if let Err(e) = res {
				log::error!("Error: {}", e);
			} else {
				let now = T::TimeProvider::now();
				log::info!("setting last sync timestamp: {}", now.as_millis());
				<LastSyncAt<T>>::put(now.as_millis());	
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

		// TO-DO
		#[pallet::weight(1000)]
		pub fn burn(
			origin: OriginFor<T>,
			transaction: Transaction
		) -> DispatchResultWithPostInfo {
			// ensure_root()
			// TO-DO
			Ok(().into())
		}

		// TO-DO
		#[pallet::weight(1000)]
		pub fn mint(
			origin: OriginFor<T>,
			transaction: Transaction
		) -> DispatchResultWithPostInfo {
			Ok(().into())
		}

		/// Issue an amount of tokens from origin
		///
		/// This is used to process incoming transactions in the bank statements
		///
		/// Params:
		/// - `iban_account` 
		///
		/// Emits: `MintedNewIban` event
		#[pallet::weight(10_000)]
		pub fn mint_batch(
			origin: OriginFor<T>,
			statements: Vec<(IbanAccount, Vec<Transaction>)>
		) -> DispatchResultWithPostInfo {
			let _who = ensure_signed(origin)?;

			log::info!("processing transactions...");
			
			for (iban_account, transactions) in statements {
				log::info!("Minting for: {:?}", iban_account.iban.clone());
				log::info!("Incoming tx: {:?}", transactions.clone());
				
				#[cfg(feature = "std")]
				Self::process_transactions(&iban_account, &transactions);
			}

			Ok(().into())
		}
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		NewBalanceEntry(IbanBalance),
		NewAccount(T::AccountId),
		Mint(T::AccountId, StrVecBytes, <<T as pallet::Config>::Currency as fungible::Inspect<T::AccountId>>::Balance),
		Burn(T::AccountId, StrVecBytes, <<T as pallet::Config>::Currency as Currency<T::AccountId>>::Balance),
		Transfer(
			T::AccountId, StrVecBytes, 
			T::AccountId, StrVecBytes, 
			<<T as pallet::Config>::Currency as Currency<T::AccountId>>::Balance)
	}

	#[pallet::validate_unsigned]
	impl<T: Config> ValidateUnsigned for Pallet<T> {
		type Call = Call<T>;
		fn validate_unsigned(_source: TransactionSource, call: &Self::Call) -> TransactionValidity {
			if let Call::mint_batch(statements) = call {
				Self::validate_tx_parameters(statements)
			} else {
				InvalidTransaction::Call.into()
			}
		}
	}

	#[pallet::storage]
	#[pallet::getter(fn iban_balances)]
	pub(super) type IbanBalances<T: Config> = StorageMap<_, Blake2_128Concat, StrVecBytes, u64, ValueQuery>;

	#[pallet::type_value]
	pub(super) fn DefaultSync<T: Config>() -> u128 { 0 }

	#[pallet::storage]
	#[pallet::getter(fn last_sync_at)]
	pub(super) type LastSyncAt<T: Config> = StorageValue<_, u128, ValueQuery, DefaultSync<T>>;

	#[pallet::type_value]
	pub(super) fn DefaultApi<T: Config>() -> StrVecBytes { API_URL.iter().cloned().collect() }	

	#[pallet::storage]
	#[pallet::getter(fn api_url)]
	pub(super) type ApiUrl<T: Config> = StorageValue<_, StrVecBytes, ValueQuery, DefaultApi<T>>;

	// mapping between internal AccountId to Iban-Account
	#[pallet::storage]
	#[pallet::getter(fn balances)]
	pub(super) type Balances<T: Config> = StorageMap<_, Blake2_128Concat, T::AccountId, IbanAccount, ValueQuery>;

	// mapping between Iban to AccountId
	#[pallet::storage]
	#[pallet::getter(fn iban_to_account)]
	pub(super) type IbanToAccount<T: Config> = StorageMap<_, Blake2_128Concat, StrVecBytes, T::AccountId, ValueQuery>;
}

/// String vector bytes
pub type StrVecBytes = Vec<u8>;

/// Iban balance type
type IbanBalance = (StrVecBytes, u64);

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug)]
pub struct Payload<Public> {
	number: u64,
	public: Public,
}

impl<T: SigningTypes> SignedPayload<T> for Payload<T::Public> {
	fn public(&self) -> T::Public {
		self.public.clone()
	}
}

/// Utility function for parsing value from json object
///
/// parse value of a given key from json object
fn parse_value(key: &str, obj: &Vec<(Vec<char>, JsonValue)>) -> JsonValue {
	let (_, v) = obj.into_iter().find(|(k, _)| k.iter().copied().eq(key.chars())).unwrap();
	v.clone()
}

#[derive(Encode, Decode, Clone, Copy, PartialEq, Eq, RuntimeDebug)]
pub enum TransactionType {
	Incoming,
	Outgoing,
	None
}

/// implement default trait for transaction type
impl Default for TransactionType {
	fn default() -> Self {
		Self::None
	}
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, Default, RuntimeDebug)]
pub struct Transaction {
	// from
	iban: StrVecBytes,
	name: StrVecBytes,
	currency: StrVecBytes,
	amount: u128,
	// to
	reference: StrVecBytes,
	tx_type: TransactionType
}

impl Transaction {
	// Get single transaction instance from json
	pub fn from_json_statement(json: &JsonValue, tx_type: &TransactionType) -> Option<Self> {
		let transaction = match json {
			JsonValue::Object(obj) => {
				// log::info!("receiving obj {:?}", obj);
				let iban = match parse_value("iban", obj) {
					JsonValue::String(str) => str.iter().map(|c| *c as u8).collect::<Vec<_>>(),
					_ => return None,
				};
				log::info!("Iban {:?}", core::str::from_utf8(&iban).unwrap_or("default"));
				let name = match parse_value("name", obj) {
					JsonValue::String(cur) => cur.iter().map(|c| *c as u8).collect::<Vec<_>>(),
					_ => return None,
				};
				log::info!("name {:?}", core::str::from_utf8(&name).unwrap_or("default"));
				let currency = match parse_value("currency", obj) {
					JsonValue::String(cur) => cur.iter().map(|c| *c as u8).collect::<Vec<_>>(),
					_ => return None,
				};
				log::info!("cur {:?}", core::str::from_utf8(&currency).unwrap_or("default"));
				let amount = match parse_value("amount", obj) {
					JsonValue::Number(num) => {
						let exp = num.fraction_length.checked_sub(2).unwrap_or(0);
						let balance = num.integer as u128 + (num.fraction / 10_u64.pow(exp)) as u128;
						balance
					},
					_ => return None,
				};
				let reference = match parse_value("reference", obj) {
					JsonValue::String(cur) => cur.iter().map(|c| *c as u8).collect::<Vec<_>>(),
					_ => return None,
				};
				
				Self {
					iban,
					name,
					currency,
					amount,
					reference,
					tx_type: *tx_type
				}
			},
			_ => return None,
		};
		Some(transaction)
	}
	
	// Parse transactions from json based on Type
	pub fn parse_transactions(json: &JsonValue, transaction_type: TransactionType) -> Option<Vec<Self>> {
		let parsed_transactions = match json {
			JsonValue::Object(obj) => {
				let transactions = match transaction_type {
					TransactionType::Incoming => {
						let incoming_transactions = match parse_value("incomingTransactions", obj) {
							JsonValue::Array(txs) => {
								txs.iter().map(|json| Self::from_json_statement(json, &transaction_type).unwrap_or(Default::default())).collect::<Vec<Transaction>>()
							}
							_ => return None,
						};
						incoming_transactions
					},
					TransactionType::Outgoing => {
						let outgoing_transactions = match parse_value("outgoingTransactions", obj) {
							JsonValue::Array(txs) => {
								txs.iter().map(|json| Self::from_json_statement(json, &transaction_type).unwrap_or(Default::default())).collect::<Vec<Transaction>>()
							}
							_ => return None,
						};
						outgoing_transactions
					},
					_ => Default::default()
				};
				transactions
			},
			_ => return None
		};
		Some(parsed_transactions)
	}
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, Default, RuntimeDebug)]
pub struct IbanAccount {
	iban: StrVecBytes,
	balance: u128,
	last_updated: u64
}

impl IbanAccount {
	// decode from JsonValue object
	pub fn from_json_value(json: &JsonValue) -> Option<Self> {
		let iban_account = match json {
			JsonValue::Object(obj) => {
				let iban = match parse_value("iban", obj) {
					JsonValue::String(str) => str.iter().map(|c| *c as u8).collect::<Vec<_>>(),
					_ => return None,
				};
				let balance = match parse_value("balanceCL", obj) {
					JsonValue::Number(num) => {
						let exp = num.fraction_length.checked_sub(2).unwrap_or(0);
						let balance = num.integer as u128 + (num.fraction / 10_u64.pow(exp)) as u128;
						balance
					},
					_ => return None,
				};

				Self {
					iban,
					balance,
					last_updated: 0
				}
			},
			_ => return None,
		};
		Some(iban_account)
	}
}

impl<T: Config> Pallet<T> {
	// checks whether we should sync in the current timestamp
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
			Ok(now) => true,
			Err(MutateStorageError::ValueFunctionFailed(RECENTLY_SENT)) => false,
			Err(MutateStorageError::ConcurrentModification(_)) => false
		}
	}

	// check if iban exists in the storage
	fn iban_exists(iban: StrVecBytes) -> bool {
		IbanToAccount::<T>::contains_key(iban)
	}

	// process transaction and make changes in the iban accounts
	fn process_transaction(
		account_id: &T::AccountId, 
		transaction: &Transaction, 
	) {
		match transaction.tx_type {
			TransactionType::Incoming => {
				let balance = <<T as pallet::Config>::Currency as fungible::Inspect<T::AccountId>>::Balance::try_from(transaction.amount);
				let unwrapped_balance = balance.unwrap_or_default();

				log::info!("minting {:?} to {:?}", &unwrapped_balance, &account_id);
				
				let res = T::Currency::mint_into(
					&account_id, 
					unwrapped_balance.clone()
				);
				match res {
					Ok(()) => Self::deposit_event(Event::Mint(account_id.clone(), transaction.iban.clone(), unwrapped_balance)),
					Err(e) => log::info!("Encountered err: {:?}", e),
				};
			},
			TransactionType::Outgoing => {
				let balance = <<T as pallet::Config>::Currency as Currency<T::AccountId>>::Balance::try_from(transaction.amount);
				let unwrapped_balance = balance.unwrap_or_default();

				if Self::iban_exists(transaction.reference.clone()) {
					log::info!("transfering: {:?} to {:?}", &unwrapped_balance, &account_id);
					let dest: T::AccountId = IbanToAccount::<T>::get(transaction.reference.clone()).into();

					// perform transfer
					let res = T::Currency::transfer(
						account_id, 
						&dest, 
						unwrapped_balance.clone(),
						ExistenceRequirement::KeepAlive
					);
					
					match res {
						Ok(()) => {
							Self::deposit_event(
								Event::Transfer(
									account_id.clone(), 
									transaction.iban.clone(),
									dest, 
									transaction.reference.clone(), 
									unwrapped_balance
								)
							)
						},
						Err(e) => log::info!("Encountered err: {:?}", e),
					}
				} else {
					log::info!("burning: {:?} to {:?}", &unwrapped_balance, &account_id);

					let res = T::Currency::burn(unwrapped_balance);
					
					let settle_res = T::Currency::settle(
						account_id,
						res,
						WithdrawReasons::TRANSFER,
						ExistenceRequirement::KeepAlive
					);
					match settle_res {
						Ok(val) => Self::deposit_event(Event::Burn(account_id.clone(), transaction.iban.clone(), unwrapped_balance)),
						Err(e) => log::info!("Encountered err burning"),
					}
				}
			},
			_ => log::info!("Transaction type not supported yet!")
		};
	}

	// process minting 
	#[cfg(feature = "std")]
	fn process_transactions(iban: &IbanAccount, transactions: &Vec<Transaction>) {
		for transaction in transactions {
			// decode account id from reference
			let encoded = core::str::from_utf8(&transaction.reference).unwrap_or("default");

			let possible_account_id = AccountId32::from_ss58check(encoded);

			// proces transaction based on the value of reference
			// if decoding returns error, we look for the iban in the pallet storage
			match possible_account_id {
				Ok(account_id) => {
					let encoded = account_id.encode();
					let account = <T::AccountId>::decode(&mut &encoded[..]).unwrap();
					Self::process_transaction(
						&account,
						transaction,
					);
					<IbanToAccount<T>>::insert(iban.iban.clone(), account);
				},
				Err(_e) => {
					// if iban exists in our storage, get accountId from there
					if Self::iban_exists(iban.iban.clone()) {
						let connected_account_id: T::AccountId = IbanToAccount::<T>::get(iban.iban.clone()).into();
						Self::process_transaction(
							&connected_account_id, 
							transaction
						);
					}
					// if no iban mapping exists in the storage, create new AccountId for Iban
					else {
						let (pair, _, _) = <crypto::Pair as sp_core::Pair>::generate_with_phrase(None);
						let encoded = sp_core::Pair::public(&pair).encode();
						let account_id = <T::AccountId>::decode(&mut &encoded[..]).unwrap();
						
						log::info!("create new account: {:?}", &account_id);

						Self::process_transaction(
							&account_id, 
							transaction,
						);
						Self::deposit_event(Event::NewAccount(account_id.clone()));

						<IbanToAccount<T>>::insert(iban.iban.clone(), account_id);
					}
				}
			}			
		}
	}

	// fetch json from the Ebics Service API using lite-json
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
			Err(e) => "Error parsing json"
		};

		let json_val = parse_json(json_str).expect("Invalid json");
		// runtime_print!("json_val {:?}", json_val);
		Ok(json_val)
	}

	fn fetch_transactions_and_send_signed() -> Result<(), &'static str> {
		log::info!("fetching statements");

		// get extrinsic signer
		let signer = Signer::<T, T::AuthorityId>::all_accounts();
		
		if !signer.can_sign() {
			return Err("No local accounts available! Please, insert your keys!")
		}

		let results = signer.send_signed_transaction(|_account| {
			// Execute call to mint
			let statements= Self::parse_statements();
			Call::mint_batch(statements)
		});
		for (acc, res) in & results {
			match res {
				Ok(()) => {
					log::info!("[{:?}] Submitted minting", acc.id)
				},
				Err(e) => log::error!("[{:?}] Failed to submit transaction: {:?}", acc.id, e),
			}
		}

		Ok(())
	}

	/// parse bank statement
	///
	/// returns:
	/// 	- iban_account: IbanAccount
	///		- incoming_txs: Vec<Transacion>
	///		- outgoing_txs: Vec<Transaction>
	fn parse_statements() -> Vec<(IbanAccount, Vec<Transaction>)> {
		// fetch json value
		let remote_url = ApiUrl::<T>::get();
		let json = Self::fetch_json(&remote_url[..]).unwrap();

		let statements = match json {
			JsonValue::Array(arr) => {
				let mut balances: Vec<(IbanAccount, Vec<Transaction>)> = Vec::with_capacity(arr.capacity());
				for val in arr.iter() {
					// extract iban account
					let iban_account = IbanAccount::from_json_value(&val).unwrap();

					log::info!("parsed acc: {:?}", iban_account);

					// extract transactions
					// currently only incoming
					let mut transactions = Transaction::parse_transactions(&val, TransactionType::Outgoing).unwrap();
					let mut incoming_transactions = Transaction::parse_transactions(&val, TransactionType::Incoming).unwrap();
					
					transactions.append(&mut incoming_transactions);
					
					balances.push((iban_account, transactions));
				}
				balances
			},
			_ => return Default::default(),
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
