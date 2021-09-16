//! A demonstration of an offchain worker that sends onchain callbacks

#![cfg_attr(not(feature = "std"), no_std)]

use core::borrow::Borrow;
use core::convert::TryInto;
use codec::{Decode, Encode};
use frame_support::{pallet_prelude::ValidTransaction};
use frame_support::traits::Get;
use lite_json::{json::{JsonValue}, json_parser::{parse_json}};
use frame_system::{offchain::{AppCrypto, CreateSignedTransaction, SignedPayload, SigningTypes, SubmitTransaction}};
use sp_core::{crypto::KeyTypeId};
use sp_runtime::AccountId32;
use sp_runtime::{MultiSignature, RuntimeDebug, offchain as rt_offchain, transaction_validity::{
		InvalidTransaction, TransactionValidity
	}};
use sp_std::vec::Vec;

#[cfg(test)]
mod tests;
/// Defines application identifier for crypto keys of this module.
///
/// Every module that deals with signatures needs to declare its unique identifier for
/// its crypto keys.
/// When an offchain worker is signing transactions it's going to request keys from type
/// `KeyTypeId` via the keystore to sign the transaction.
/// The keys can be inserted manually via RPC (see `author_insertKey`).
pub const KEY_TYPE: KeyTypeId = KeyTypeId(*b"demo");

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

	pub struct TestAuthId;
	// implemented for ocw-runtime
	impl frame_system::offchain::AppCrypto<MultiSigner, MultiSignature> for TestAuthId {
		type RuntimeAppPublic = Public;
		type GenericSignature = sp_core::sr25519::Signature;
		type GenericPublic = sp_core::sr25519::Public;
	}

	// implemented for mock runtime in test
	impl frame_system::offchain::AppCrypto<<Sr25519Signature as Verify>::Signer, Sr25519Signature>
		for TestAuthId
	{
		type RuntimeAppPublic = Public;
		type GenericSignature = sp_core::sr25519::Signature;
		type GenericPublic = sp_core::sr25519::Public;
	}
}

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
	use frame_support::{pallet_prelude::*};
	use frame_system::pallet_prelude::*;
	use super::*;

	/// This is the pallet's configuration trait
	#[pallet::config]
	pub trait Config: pallet_timestamp::Config + frame_system::Config + CreateSignedTransaction<Call<Self>> {
		/// The identifier type for an offchain worker.
		type AuthorityId: AppCrypto<Self::Public, Self::Signature>;
		/// The overarching dispatch call type.
		type Call: From<Call<Self>>;
		/// The overarching event type.
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

		// Constant that defines the interval between two unsigned transactions	
		// #[pallet::constant]
		// type GracePeriod: Get<Self::BlockNumber>;

		// This ensures that we only accept unsigned transactions once, every `UnsignedInterval` blocks.
		#[pallet::constant]
		type UnsignedInterval: Get<Self::BlockNumber>;
		
		/// A configuration for base priority of unsigned transactions.
		///
		/// This is exposed so that it can be tuned for particular runtime, when
		/// multiple pallets send unsigned transactions.
		#[pallet::constant]
		type UnsignedPriority: Get<TransactionPriority>;
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

			let should_sync = Self::should_sync(&block_number);

			if !should_sync {
				return ;
			}
			let res = Self::fetch_transactions_and_send_signed(&block_number);
			// let res = Self::fetch_iban_balance_and_send_unsigned(block_number);

			if let Err(e) = res {
				log::error!("Error: {}", e);
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

		/// Issue an amount of tokens from origin
		///
		/// This is used to process incoming transactions in the bank statements
		///
		/// Params:
		/// - `iban_account` 
		///
		/// Emits: `MintedNewIban` event
		#[pallet::weight(10_000)]
		pub fn mint(
			origin: OriginFor<T>,
			statements: Vec<(IbanAccount, Vec<Transaction>)>
		) -> DispatchResultWithPostInfo {
			let _who = ensure_signed(origin)?;

			log::info!("processing incoming transactions...");
			
			for (iban_account, incoming_transactions) in statements {
				log::info!("Minting for: {:?}", iban_account.iban.clone());
				log::info!("Incoming tx: {:?}", incoming_transactions.clone());
				Self::process_minting(&iban_account, &incoming_transactions);
			}

			let current_block = <frame_system::Pallet<T>>::block_number();
			<NextSyncAt<T>>::put(current_block + T::UnsignedInterval::get());
			Ok(().into())
		}
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		NewBalanceEntry(IbanBalance),
		NewAccount(T::AccountId),
	}

	#[pallet::validate_unsigned]
	impl<T: Config> ValidateUnsigned for Pallet<T> {
		type Call = Call<T>;
		fn validate_unsigned(source: TransactionSource, call: &Self::Call) -> TransactionValidity {
			let current_block = <frame_system::Pallet<T>>::block_number();
			if let Call::mint(statements) = call {
				Self::validate_tx_parameters(&current_block, statements)
			} else {
				InvalidTransaction::Call.into()
			}
		}
	}

	#[pallet::storage]
	#[pallet::getter(fn iban_balances)]
	pub(super) type IbanBalances<T: Config> = StorageMap<_, Blake2_128Concat, StrVecBytes, u64, ValueQuery>;

	#[pallet::type_value]
	pub(super) fn DefaultSync<T: Config>() -> T::BlockNumber { 1u32.into() }

	#[pallet::storage]
	#[pallet::getter(fn next_sync_at)]
	pub(super) type NextSyncAt<T: Config> = StorageValue<_, T::BlockNumber, ValueQuery, DefaultSync<T>>;

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

/// String vectors
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

pub enum TransactionType {
	Incoming,
	Outgoing,
	None
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, Default, RuntimeDebug)]
pub struct Transaction {
	iban: StrVecBytes,
	name: StrVecBytes,
	addr_line: Vec<StrVecBytes>,
	currency: StrVecBytes,
	amount: u64,
	reference: StrVecBytes,
	pmt_inf_id: StrVecBytes,
	msg_id: StrVecBytes,
	instr_id: StrVecBytes
}

impl Transaction {	
	// get reference as AccountId32
	fn get_reference(&self) -> AccountId32 {
		let account_u8: [u8; 32] =  self.reference.clone()
			.try_into()
			.unwrap_or_else(|v: Vec<u8>| panic!("Expected a Vec of length {} but it was {}", 32, v.len()));
		let account = sp_core::crypto::AccountId32::from(account_u8);
		account
	}

	// Get single transaction instance from json
	pub fn from_json_statement(json: &JsonValue) -> Option<Self> {
		let transaction = match json {
			JsonValue::Object(obj) => {
				// log::info!("receiving obj {:?}", obj);
				let iban = match parse_value("iban", obj) {
					JsonValue::String(str) => str.iter().map(|c| *c as u8).collect::<Vec<_>>(),
					_ => return None,
				};
				log::info!("Iban {:?}", iban);
				let name = match parse_value("name", obj) {
					JsonValue::String(cur) => cur.iter().map(|c| *c as u8).collect::<Vec<_>>(),
					_ => return None,
				};
				log::info!("name {:?}", name);
				let currency = match parse_value("currency", obj) {
					JsonValue::String(cur) => cur.iter().map(|c| *c as u8).collect::<Vec<_>>(),
					_ => return None,
				};
				log::info!("currency {:?}", currency);
				let amount = match parse_value("amount", obj) {
					JsonValue::Number(num) => {
						let exp = num.fraction_length.checked_sub(2).unwrap_or(0);
						let balance = num.integer as u64 * 100 + (num.fraction / 10_u64.pow(exp)) as u64;
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
					..Default::default()
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
								txs.iter().map(|json| Self::from_json_statement(json).unwrap_or(Default::default())).collect::<Vec<Transaction>>()
							}
							_ => return None,
						};
						incoming_transactions
					},
					TransactionType::Outgoing => {
						let outgoing_transactions = match parse_value("outgoingTransactions", obj) {
							JsonValue::Array(txs) => {
								txs.iter().map(|json| Self::from_json_statement(json).unwrap_or(Default::default())).collect::<Vec<Transaction>>()
							}
							_ => return None,
						};
						outgoing_transactions
					},
					_ => Default::default()
				};
				log::info!("parsed txs: {:?}", transactions);
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
	balance_op: u64,
	balance_op_currency: StrVecBytes,
	balance_cl: u64,
	balance_cl_currency: StrVecBytes,
	booking_date: StrVecBytes,
	// validation_date: StrVecBytes
	// incoming_transactions: Vec<Transaction>,
	// outgoing_transactions: Vec<Transaction>
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
				let balance_op = match parse_value("balanceOP", obj) {
					JsonValue::Number(num) => {
						let exp = num.fraction_length.checked_sub(2).unwrap_or(0);
						let balance = num.integer as u64 * 100 + (num.fraction / 10_u64.pow(exp)) as u64;
						balance
					},
					_ => return None,
				};
				let balance_op_currency = match parse_value("balanceOPCurrency", obj) {
					JsonValue::String(cur) => cur.iter().map(|c| *c as u8).collect::<Vec<_>>(),
					_ => return None,
				};
				let balance_cl = match parse_value("balanceCL", obj) {
					JsonValue::Number(num) => {
						let exp = num.fraction_length.checked_sub(2).unwrap_or(0);
						let balance = num.integer as u64 * 100 + (num.fraction / 10_u64.pow(exp)) as u64;
						balance
					},
					_ => return None,
				};
				let balance_cl_currency = match parse_value("balanceCLCurrency", obj) {
					JsonValue::String(cur) => cur.iter().map(|c| *c as u8).collect::<Vec<_>>(),
					_ => return None,
				};
				// let balance_cl_date = match parse_value("balanceCLDate", obj) {
				// 	JsonValue::String(date) => date.iter().map(|c| *c as u8).collect::<Vec<_>>(),
				// 	_ => return None,
				// };
				let booking_date = match parse_value("bookingDate", obj) {
					JsonValue::String(date) => date.iter().map(|c| *c as u8).collect::<Vec<_>>(),
					_ => return None,
				};
				// let validation_date = match parse_value("validationDate", obj) {
				// 	JsonValue::String(date) => date.iter().map(|c| *c as u8).collect::<Vec<_>>(),
				// 	_ => return None,
				// };

				Self {
					iban,
					balance_op,
					balance_op_currency,
					balance_cl,
					balance_cl_currency,
					booking_date,
				}
			},
			_ => return None,
		};
		Some(iban_account)
	}
}

impl<T: Config> Pallet<T> {
	// checks whether we should sync in this block number
	fn should_sync(block_number: &T::BlockNumber) -> bool {
		let next_sync_at = <NextSyncAt<T>>::get();
		if &next_sync_at == block_number {
			return true
		}
		false
	}

	// check if iban exists in the storage
	fn iban_exists(iban: StrVecBytes) -> bool {
		IbanToAccount::<T>::contains_key(iban)
	}

	// process transactio and make changes in the iban accounts
	fn process_transaction(
		iban_account: IbanAccount, 
		transaction: &Transaction, 
		transaction_type: &TransactionType
	) -> IbanAccount {
		let new_iban_account = match transaction_type {
			TransactionType::Incoming => {
				let new_op_balance = iban_account.balance_cl;
				let new_cl_balance = iban_account.balance_cl + transaction.amount;
				IbanAccount {
					balance_op: new_op_balance,
					balance_cl: new_cl_balance,
					..iban_account
				}
			},
			_ => return iban_account
		};
		new_iban_account
	}

	// process minting 
	fn process_minting(iban: &IbanAccount, incoming_transactions: &Vec<Transaction>) {
		// if transaction is coming from
		for incoming_transaction in incoming_transactions {
			// if iban exists in our storage, get accountId from there
			if Self::iban_exists(iban.iban.clone()) {
				let connected_account_id: T::AccountId = IbanToAccount::<T>::get(iban.iban.clone()).into();
				let old_iban_account = Balances::<T>::get(connected_account_id.borrow());
				let new_iban_account = Self::process_transaction(
					old_iban_account, 
					incoming_transaction, 
					&TransactionType::Incoming
				);
				Balances::<T>::insert(connected_account_id, new_iban_account);
			}
			else {
				let encoded = incoming_transaction.reference.encode();
				// let possible_account = sp_std::str::from_utf8(&incoming_transaction.reference[..]).unwrap();
				let account_id = <T::AccountId>::decode(&mut &encoded[..]).unwrap();
				let old_iban_account = Balances::<T>::get(account_id.clone());
				let new_iban_account = Self::process_transaction(
					old_iban_account, 
					incoming_transaction, 
					&TransactionType::Incoming
				);
				Balances::<T>::insert(account_id, new_iban_account);
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

	fn fetch_transactions_and_send_signed(
		block_number: &T::BlockNumber
	) -> Result<(), &'static str> {
		log::info!("fetching statements");

		let next_sync_at = NextSyncAt::<T>::get();

		if &next_sync_at > block_number {
			return Err("Too early to send signed transaction")
		}

		let statements= Self::parse_statements();

		let call = Call::mint(statements);
		SubmitTransaction::<T, Call<T>>::submit_unsigned_transaction(call.into())
			.map_err(|()| "Unable to submit tx")?;
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
					// let outgoing_transactions = Transaction::parse_transactions(&json, TransactionType::Outgoing);
					let incoming_transactions = Transaction::parse_transactions(&val, TransactionType::Incoming).unwrap();

					balances.push((iban_account, incoming_transactions));
				}
				balances
			},
			_ => return Default::default(),
		};
		statements
	}

	fn validate_tx_parameters(
		block_number: &T::BlockNumber,
		statements: &Vec<(IbanAccount, Vec<Transaction>)>
	) -> TransactionValidity {
		// check if we are on time
		let next_sync_at = <NextSyncAt<T>>::get();
		if &next_sync_at > block_number {
			return InvalidTransaction::Stale.into()
		}

		let current_block = <frame_system::Pallet<T>>::block_number();
		if &current_block < block_number {
			return InvalidTransaction::Future.into()
		}

		ValidTransaction::with_tag_prefix("FiatRamps")
			.priority(T::UnsignedPriority::get().saturating_add(statements.capacity() as u64))
			.and_provides(next_sync_at)
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
