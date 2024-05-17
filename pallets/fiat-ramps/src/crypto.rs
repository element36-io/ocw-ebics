//! Cryptographic utilities for Fiat Ramps.

/// Based on the above `KeyTypeId` we need to generate a pallet-specific crypto type wrapper.
/// We can utilize the supported crypto kinds (`sr25519`, `ed25519` and `ecdsa`) and augment
/// them with the pallet-specific identifier.
use crate::KEY_TYPE;
use sp_core::sr25519::Signature as Sr25519Signature;
use sp_runtime::{
	app_crypto::{app_crypto, sr25519},
	traits::Verify,
	MultiSignature, MultiSigner,
};
use sp_std::convert::TryFrom;

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
