//! Types of this pallet
use crate::*;
use frame_support::PalletId;
use sp_runtime::KeyTypeId;
use sp_std::default::Default;

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

/// Hardcoded inital test api endpoint
pub const API_URL: &[u8; 27] = b"http://localhost:8093/ebics";

/// Account id of
pub type AccountIdOf<T> = <T as frame_system::Config>::AccountId;

/// Balance type
pub type BalanceOf<T> = <<T as Config>::Currency as Currency<AccountIdOf<T>>>::Balance;

/// String vector
pub type BoundedString<MaxLength> = BoundedVec<u8, MaxLength>;

/// String vector bytes
pub type StringOf<T> = BoundedString<<T as Config>::MaxStringLength>;

/// IBAN representation
pub type Iban<MaxLength> = BoundedVec<u8, MaxLength>;

/// IBAN account
pub type IbanOf<T> = Iban<<T as Config>::MaxIbanLength>;

/// Transaction type
pub type TransactionOf<T> =
	Transaction<<T as Config>::MaxIbanLength, <T as Config>::MaxStringLength>;

/// Bank account type
pub type BankAccountOf<T> = BankAccount<<T as Config>::MaxIbanLength>;

/// Transfer destination of `Config`
pub type TransferDestinationOf<T> =
	TransferDestination<<T as Config>::MaxIbanLength, <T as frame_system::Config>::AccountId>;

/// Explicit `BoundedVec<Statement>` type alias
pub type StatementsOf<T> = BoundedVec<
	(BankAccountOf<T>, BoundedVec<TransactionOf<T>, <T as Config>::MaxStatements>),
	<T as Config>::MaxStatements,
>;

/// Types of activities that can be performed by the OCW
#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, Default)]
pub enum OcwActivity {
	FetchStatements,
	ProcessBurnRequests,
	VerifyAndProcessStatements,
	#[default]
	None,
}

/// Type that represents a burn request
#[derive(Clone, Encode, Decode, Eq, PartialEq, RuntimeDebug, MaxEncodedLen, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub struct BurnRequest<MaxLength: Get<u32>, Balance: MaxEncodedLen> {
	pub id: u64,
	pub burner: Iban<MaxLength>,
	pub dest_iban: Iban<MaxLength>,
	pub amount: Balance,
}

/// Trait for deseralizing a value from a JsonValue type
pub trait Deserialize<T> {
	fn deserialize(value: &JsonValue) -> Option<T>;
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
pub(crate) struct Payload<Public> {
	pub number: u64,
	pub public: Public,
}

/// Type of transaction
#[derive(Encode, Decode, Clone, Copy, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub enum TransactionType {
	Incoming,
	Outgoing,
	None,
}

/// Representation of transaction in EBICS format
#[derive(Encode, Decode, Clone, PartialEq, Eq, Default, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct Transaction<MaxLength: Get<u32>, StringMaxLength: Get<u32>> {
	/// IBAN of the sender
	pub iban: Iban<MaxLength>,
	/// Name of the sender
	pub name: BoundedString<StringMaxLength>,
	/// Currency of the transaction
	pub currency: BoundedString<StringMaxLength>,
	/// Amount of the transaction
	pub amount: u128,
	/// Reference of the transaction, usually includes recipient's `AccountId`
	pub reference: BoundedString<StringMaxLength>,
	/// Type of the transaction: incoming or outgoing
	pub tx_type: TransactionType,
}

/// Representation of a Bank Account
#[derive(Encode, Decode, Clone, PartialEq, Eq, Default, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct BankAccount<MaxLength: Get<u32>> {
	/// IBAN number of the account
	pub iban: Iban<MaxLength>,
	/// Closing balance of the account
	pub balance: u128,
	/// Last block the statement was updated
	pub last_updated: u64,
}

/// Burn destination
#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo)]
pub enum TransferDestination<MaxLength: Get<u32>, AccountId> {
	/// Burn to a specific IBAN
	Iban(Iban<MaxLength>),
	/// Burn to another account, i.e transfer on-chain
	Address(AccountId),
	/// Withdraw
	Withdraw,
}

/// Information about the queued statements
#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, MaxEncodedLen, TypeInfo)]
pub struct QueuedStatementsInfo<BlockNumber, Statements, ReceiptUrl> {
	/// Block number when the statements were queued
	pub block_number: BlockNumber,
	/// List of statements
	pub statements: Statements,
	/// URL for the receipt of the statements
	pub receipt_url: ReceiptUrl,
}

pub type QueuedStatementsInfoOf<T> = QueuedStatementsInfo<
	BlockNumberFor<T>,
	StatementsOf<T>,
	BoundedString<<T as Config>::MaxStringLength>,
>;
