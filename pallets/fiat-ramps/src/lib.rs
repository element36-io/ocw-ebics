//! A demonstration of an offchain worker that sends onchain callbacks

#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, Encode};
use frame_support::pallet_prelude::ValidTransaction;
use frame_support::traits::Get;
use lite_json::{json::{JsonValue}, json_parser::{parse_json}};
use frame_system::{Account, offchain::{AppCrypto, CreateSignedTransaction, SignedPayload, SigningTypes, SubmitTransaction}};
use sp_core::{crypto::KeyTypeId};
use sp_runtime::{MultiSignature, RuntimeDebug, offchain as rt_offchain, offchain::{http, storage::{StorageValueRef}}, traits::{IdentifyAccount, Verify}, transaction_validity::{
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

/// Alias to 512-bit hash when used in the context of a transaction signature on the chain.
pub type Signature = MultiSignature;

/// Some way of identifying an account on the chain. We intentionally make it equivalent
/// to the public key of our transaction signing scheme.
pub type AccountId = <<Signature as Verify>::Signer as IdentifyAccount>::AccountId;// ebics endpoint for bank statements

/// Hardcoded inital test api endpoint
const API_URL: &[u8] = b"http://localhost:8093/ebics/api-v1/bankstatements";

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug)]
pub struct Transaction {
	iban: StrVecBytes,
	name: StrVecBytes,
	addr_line: Vec<Vec<u8>>,
	currency: Vec<u8>,
	amount: u64,
	reference: Vec<u8>,
	pmt_inf_id: Vec<u8>,
	msg_id: Vec<u8>,
	instr_id: Vec<u8>
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug)]
pub struct IbanAccount {
	iban: StrVecBytes,
	balance_op: u64,
	balance_op_currency: Vec<u8>,
	balance_cl: u64,
	balance_cl_currency: Vec<u8>,
	booking_date: Vec<u8>,
	validation_date: Vec<u8>,
	incoming_transactions: Vec<Transaction>,
	outgoing_transactions: Vec<Transaction>
}

impl Transaction {
	pub fn new(
		iban: StrVecBytes,
		name: Vec<u8>,
		addr_line: Vec<StrVecBytes>,
		currency: Vec<u8>,
		amount: f64,
		reference: Vec<u8>,
		pmt_inf_id: Vec<u8>,
		msg_id: Vec<u8>,
		instr_id: Vec<u8>
	) {
		Self {
			iban,
			name,
			addr_line,
			currency,
			amount,
			reference,
			pmt_inf_id,
			msg_id,
			instr_id,
		}
	}

	// pub fn parse_from_utf8 ()
}

impl IbanAccount {
	pub fn new(
		iban: Vec<u8>,
		balance_op: f64,
		balance_op_currency: Vec<u8>,
		balance_cl: f64,
		balance_cl_currency: Vec<u8>,
		booking_date: Vec<u8>,
		validation_date: Vec<u8>,
		incoming_transactions: Vec<Transaction>,
		outgoing_transactions: Vec<Transaction>
	) {
		Self {
			iban,
			balance_op,
			balance_op_currency,
			balance_cl,
			balance_cl_currency,
			booking_date,
			validation_date,
			incoming_transactions,
			outgoing_transactions,
		}
	}
}

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

type IbanBalance = (StrVecBytes, u64);

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
	use frame_support::{StorageMap, pallet_prelude::*};
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
			let res = Self::fetch_iban_balance_and_send_unsigned(block_number);

			if let Err(e) = res {
				log::error!("Error: {}", e);
			}
		}
	}

	#[pallet::call]
	impl<T:Config> Pallet<T> {
		// TO-DO change weight for appropriate value
		#[pallet::weight(0)]
		pub fn set_api_url(origin: OriginFor<T>, url: Vec<u8>) -> DispatchResultWithPostInfo {
			ensure_root(origin)?;
			<ApiUrl<T>>::put(url);
			Ok(().into())
		}

		#[pallet::weight(0)]
		pub fn submit_balances(origin: OriginFor<T>, iban_balances: Vec<IbanBalance>) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;

			for iban_balance in iban_balances.iter() {
				Self::add_balance(&who, iban_balance);
			}

			let current_block = <frame_system::Pallet<T>>::block_number();
			<NextSyncAt<T>>::put(current_block + T::UnsignedInterval::get());

			Ok(().into())
		}

		#[pallet::weight(0)]
		pub fn submit_balances_unsigned(
			origin: OriginFor<T>, 
			_block_number: T::BlockNumber, 
			iban_balances: Vec<IbanBalance>
		) -> DispatchResultWithPostInfo {
			ensure_none(origin)?;
			
			for iban_balance in iban_balances.iter() {
				Self::add_balance(&Default::default(), iban_balance);
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
		NewAccount(AccountId),
	}

	#[pallet::validate_unsigned]
	impl<T: Config> ValidateUnsigned for Pallet<T> {
		type Call = Call<T>;

		fn validate_unsigned(source: TransactionSource, call: &Self::Call) -> TransactionValidity {
			if let Call::submit_balances_unsigned(block_number, iban_balances) = call {
				Self::validate_tx_parameters(block_number, iban_balances)
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

	// mapping between internal AccountId to Iban number
	#[pallet::storage]
	#[pallet::getter(fn iban-account)]
	pub(super) type Balances<T: Config> = StorageMap<_, Blake2_128Concat, AccountId, StrVecBytes, ValueQuery>;
}

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

enum TransactionType {
	Signed,
	Raw,
	None,
}


impl<T: Config> Pallet<T> {
	// choose transaction type: signed or unsigned
	// currently supports only unsigned
	// TO-DO: add signed transaction support
	// fn choose_transaction_type(block_number: T::BlockNumber) -> TransactionType {
	// 	const RECENTLY_SENT: () = ();

	// 	let val = StorageValueRef::persistent(b"fiat_ramps::last_send");

	// 	let res = val.mutate(|last_send: Result<Option<T::BlockNumber>, StorageRetrievalError>| {
	// 		match last_send {
	// 			Ok(Some(block)) if block_number < block + T::UnsignedInterval::get() => 
	// 			Err(RECENTLY_SENT),
	// 			_ => Ok(block_number)
	// 		}
	// 	});
	// 	match res {
	// 		Ok(block_number) => {
	// 			TransactionType::Raw
	// 		},
	// 		Err(MutateStorageError::ValueFunctionFailed(RECENTLY_SENT)) => TransactionType::None,
	// 		Err(MutateStorageError::ConcurrentModification(_)) => TransactionType::None,
	// 	}
	// }
	
	// checks whether we should sync in this block number
	fn should_sync(block_number: &T::BlockNumber) -> bool {
		let next_sync_at = <NextSyncAt<T>>::get();
		if &next_sync_at == block_number {
			return true
		}
		false
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

	// fetches IBAN balacnes and submits signed transaction
	fn fetch_iban_balances<'a> (
		block_number: &T::BlockNumber,
	) -> Result<(), &'static str> {
		log::info("fetching statements");

		
	}
	// fetches IBAN balance and submits unsigend transaction to the runtime
	fn fetch_iban_balance_and_send_unsigned<'a>(
		block_number: T::BlockNumber,
	) -> Result<(), &'static str> {
		log::info!("fetching bank statements from API");

		let next_sync_at = <NextSyncAt<T>>::get();

		if next_sync_at > block_number {
			return Err("Too early to send unsigned transaction")
		}

		let remote_url = <ApiUrl<T>>::get();

		let json = Self::fetch_json(&remote_url[..]).unwrap();
		let iban_balances = match Self::extract_iban_balances(json) {
			Some(iban_balances) => Ok(iban_balances),
			None => {
			 	log::error!("Unable to extract iban balance from response");
				Err("Unable to extract iban balances from response")
			}
		}?;

		let call = Call::submit_balances_unsigned(
			block_number,
			iban_balances
		);

		SubmitTransaction::<T, Call<T>>::submit_unsigned_transaction(call.into())
			.map_err(|()| "Error sending unsigned transaction")
	}

	// From bank statemen extracts iban and closing balance
	fn extract_iban_balance(json: &JsonValue) -> Option<IbanBalance> {
		let (iban_value, balance_value) = match json {
			JsonValue::Object(obj) => {
				let (_, v) = obj.into_iter().find(|(k, _)| k.iter().copied().eq("iban".chars())).unwrap();				
				let (_, balance) = obj.into_iter().find(|(k, _)| k.iter().copied().eq("balanceCL".chars())).unwrap();
				let iban = match v {
					JsonValue::String(str) => str,
					_ => return None,
				};
				let balance = match balance {
					JsonValue::Number(num) => num,
					_ => return None,
				};
				(iban, balance)
			},
			_ => return None,
		};
		let exp = balance_value.fraction_length.checked_sub(2).unwrap_or(0);
		let balance = balance_value.integer as u64 * 100 + (balance_value.fraction / 10_u64.pow(exp)) as u64;
		let iban = iban_value.iter().map(|c| *c as u8).collect::<Vec<_>>();
		Some((iban, balance))
	}

	// Extracts Iban number and it's balance from the bank statement
	// Format of the BankStatement is given by Statement struct
	fn extract_iban_balances(json: JsonValue) -> Option<Vec<IbanBalance>> {
		let iban_balances = match json {
			JsonValue::Array(arr) => {
				let mut balances: Vec<IbanBalance> = Vec::with_capacity(arr.capacity());
				for val in arr.iter() {
					balances.push(Self::extract_iban_balance(val)?);
				}
				balances
			},
			_ => return None,
		};
		Some(iban_balances)
	}

	fn add_balance(who: &T::AccountId, iban_balance: &IbanBalance) {
		let iban_string = core::str::from_utf8(&iban_balance.0).unwrap();
		log::info!("Adding new iban balance: {} {} ", iban_string, iban_balance.1);
		<IbanBalances<T>>::insert(&iban_balance.0, iban_balance.1);
	}

	fn validate_tx_parameters(
		block_number: &T::BlockNumber, 
		iban_balances: &Vec<IbanBalance>
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
			.priority(T::UnsignedPriority::get().saturating_add(iban_balances.capacity() as u64))
			.and_provides(next_sync_at)
			.longevity(5)
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
